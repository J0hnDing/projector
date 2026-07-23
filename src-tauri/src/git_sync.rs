use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use git2::Repository;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::models::{GitFetchStatus, RegistryEntry};

const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Default)]
pub struct GitFetchSnapshot {
    pub status: GitFetchStatus,
    pub last_successful_fetch: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct FetchCache {
    projects: HashMap<Uuid, DateTime<Utc>>,
}

#[derive(Clone)]
pub struct GitSyncManager {
    inner: Arc<GitSyncInner>,
}

struct GitSyncInner {
    states: Mutex<HashMap<Uuid, GitFetchSnapshot>>,
    active: Mutex<HashSet<Uuid>>,
    cache_path: PathBuf,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GitSyncChangedEvent {
    project_id: Uuid,
}

impl GitSyncManager {
    pub fn load(cache_path: PathBuf) -> Self {
        let cache = fs::read(&cache_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<FetchCache>(&bytes).ok())
            .unwrap_or_default();
        let states = cache
            .projects
            .into_iter()
            .map(|(id, last_successful_fetch)| {
                (
                    id,
                    GitFetchSnapshot {
                        last_successful_fetch: Some(last_successful_fetch),
                        ..GitFetchSnapshot::default()
                    },
                )
            })
            .collect();
        Self {
            inner: Arc::new(GitSyncInner {
                states: Mutex::new(states),
                active: Mutex::new(HashSet::new()),
                cache_path,
            }),
        }
    }

    pub fn snapshot(&self, id: Uuid) -> GitFetchSnapshot {
        self.inner
            .states
            .lock()
            .ok()
            .and_then(|states| states.get(&id).cloned())
            .unwrap_or_default()
    }

    pub fn remove(&self, id: Uuid) {
        if let Ok(mut states) = self.inner.states.lock() {
            states.remove(&id);
            persist_cache(&self.inner.cache_path, &states);
        }
    }

    pub fn fetch(&self, app: AppHandle, entry: RegistryEntry) {
        if !is_fetchable_repository(&entry.path) {
            return;
        }

        let Ok(mut active) = self.inner.active.lock() else {
            return;
        };
        if !active.insert(entry.id) {
            return;
        }
        drop(active);

        self.update_state(
            entry.id,
            |state| {
                state.status = GitFetchStatus::Fetching;
                state.error = None;
            },
            false,
        );
        let _ = app.emit(
            "git-sync-changed",
            GitSyncChangedEvent {
                project_id: entry.id,
            },
        );

        let manager = self.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let outcome = fetch_repository(&entry.path, FETCH_TIMEOUT);
            manager.finish_fetch(entry.id, outcome);
            let _ = app.emit(
                "git-sync-changed",
                GitSyncChangedEvent {
                    project_id: entry.id,
                },
            );
        });
    }

    fn finish_fetch(&self, id: Uuid, outcome: FetchOutcome) {
        self.update_state(
            id,
            |state| {
                state.status = outcome.status;
                state.error = outcome.error;
                if state.status == GitFetchStatus::Succeeded {
                    state.last_successful_fetch = Some(Utc::now());
                }
            },
            true,
        );
        if let Ok(mut active) = self.inner.active.lock() {
            active.remove(&id);
        }
    }

    fn update_state(&self, id: Uuid, update: impl FnOnce(&mut GitFetchSnapshot), persist: bool) {
        if let Ok(mut states) = self.inner.states.lock() {
            update(states.entry(id).or_default());
            if persist {
                persist_cache(&self.inner.cache_path, &states);
            }
        }
    }
}

fn persist_cache(path: &Path, states: &HashMap<Uuid, GitFetchSnapshot>) {
    let projects = states
        .iter()
        .filter_map(|(id, state)| state.last_successful_fetch.map(|time| (*id, time)))
        .collect();
    let cache = FetchCache { projects };
    let Ok(bytes) = serde_json::to_vec_pretty(&cache) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, bytes);
}

fn is_fetchable_repository(root: &Path) -> bool {
    let Ok(repo) = Repository::open(root) else {
        return false;
    };
    repository_is_within_root(&repo, root)
}

fn repository_is_within_root(repo: &Repository, root: &Path) -> bool {
    root.canonicalize()
        .ok()
        .zip(repo.path().canonicalize().ok())
        .is_some_and(|(root, git_dir)| git_dir.starts_with(root))
}

#[derive(Debug)]
struct FetchOutcome {
    status: GitFetchStatus,
    error: Option<String>,
}

fn fetch_repository(root: &Path, timeout: Duration) -> FetchOutcome {
    let repo = match Repository::open(root) {
        Ok(repo) => repo,
        Err(error) => {
            return failed(format!("The repository is unavailable: {error}"));
        }
    };
    if !repository_is_within_root(&repo, root) {
        return failed("Git metadata is outside the registered project directory".into());
    }
    match repo.remotes() {
        Ok(remotes) if remotes.is_empty() => {
            return FetchOutcome {
                status: GitFetchStatus::NoRemote,
                error: Some("No Git remotes are configured.".into()),
            };
        }
        Err(error) => return failed(format!("Unable to inspect Git remotes: {error}")),
        _ => {}
    }

    let mut child = match Command::new("git")
        .args(["fetch", "--all", "--prune"])
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return failed(format!("Unable to start Git fetch: {error}")),
    };

    let stderr = child.stderr.take();
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut stderr) = stderr {
            let _ = stderr.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).trim().to_string()
    });
    let started = Instant::now();
    let exit_status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return failed(format!("Unable to monitor Git fetch: {error}"));
            }
        }
    };
    if exit_status.is_none() {
        // A credential helper or SSH process may still hold the pipe after Git is
        // killed. Detaching the reader keeps the configured timeout authoritative.
        drop(stderr_reader);
        return FetchOutcome {
            status: GitFetchStatus::TimedOut,
            error: Some(format!(
                "Git fetch timed out after {} seconds.",
                timeout.as_secs()
            )),
        };
    }
    let stderr = stderr_reader.join().unwrap_or_default();

    match exit_status {
        Some(status) if status.success() => FetchOutcome {
            status: GitFetchStatus::Succeeded,
            error: None,
        },
        Some(_) => classify_fetch_failure(&stderr),
        None => unreachable!("the timeout case returns before reading stderr"),
    }
}

fn classify_fetch_failure(stderr: &str) -> FetchOutcome {
    let lower = stderr.to_lowercase();
    let status = if [
        "authentication failed",
        "could not read username",
        "permission denied (publickey)",
        "invalid username or password",
        "terminal prompts disabled",
        "credential",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        GitFetchStatus::AuthenticationFailed
    } else if [
        "could not resolve host",
        "failed to connect",
        "network is unreachable",
        "network unreachable",
        "connection timed out",
        "couldn't connect to server",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        GitFetchStatus::Offline
    } else {
        GitFetchStatus::Failed
    };
    let error = match status {
        GitFetchStatus::AuthenticationFailed => {
            "Git authentication failed. Check this repository's credentials and try again.".into()
        }
        GitFetchStatus::Offline => {
            "Git could not reach the remote. Check the network connection and try again.".into()
        }
        _ if stderr.is_empty() => "Git fetch failed without additional details.".into(),
        _ => "Git fetch failed. Run git fetch in the repository for details.".into(),
    };
    FetchOutcome {
        status,
        error: Some(error),
    }
}

fn failed(message: String) -> FetchOutcome {
    FetchOutcome {
        status: GitFetchStatus::Failed,
        error: Some(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_authentication_and_offline_failures() {
        assert_eq!(
            classify_fetch_failure("fatal: Authentication failed").status,
            GitFetchStatus::AuthenticationFailed
        );
        assert_eq!(
            classify_fetch_failure("fatal: unable to access: Could not resolve host").status,
            GitFetchStatus::Offline
        );
        assert_eq!(
            classify_fetch_failure("fatal: an unexpected failure").status,
            GitFetchStatus::Failed
        );
    }

    #[test]
    fn missing_remote_is_reported_without_running_fetch() {
        let temp = tempfile::tempdir().unwrap();
        Repository::init(temp.path()).unwrap();
        let outcome = fetch_repository(temp.path(), Duration::from_secs(1));
        assert_eq!(outcome.status, GitFetchStatus::NoRemote);
        assert!(outcome.error.unwrap().contains("No Git remotes"));
    }
}
