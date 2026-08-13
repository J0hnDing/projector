use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use git2::{BranchType, ErrorCode, Repository, StatusOptions};

use crate::git_sync::GitFetchSnapshot;
use crate::models::{
    DocumentContent, DocumentStatus, GitCommit, GitInfo, GitSyncStatus, ProjectDetail,
    ProjectDocuments, ProjectState, ProjectSummary, RegistryEntry, TodoDocument, ValidationWarning,
    WorkHistoryDocument,
};

const MAX_DOCUMENT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_COMMITS: usize = 25;

pub fn summarize(entry: &RegistryEntry, fetch: &GitFetchSnapshot) -> ProjectSummary {
    ProjectSummary {
        id: entry.id,
        name: entry.name.clone(),
        path: entry.path.clone(),
        registered_at: entry.registered_at,
        last_opened: entry.last_opened,
        git: observe_git(&entry.path, fetch),
    }
}

pub fn detail(entry: &RegistryEntry, fetch: &GitFetchSnapshot) -> ProjectDetail {
    ProjectDetail {
        project: summarize(entry, fetch),
        documents: observe_documents(&entry.path),
        state: crate::project_state::inspect_project_state(&entry.path).unwrap_or_else(|error| {
            ProjectState {
                todos: TodoDocument {
                    relative_path: None,
                    items: Vec::new(),
                    warnings: vec![ValidationWarning {
                        code: "state_unavailable".into(),
                        message: error.to_string(),
                        item_id: None,
                    }],
                    preserved_content: None,
                },
                working_history: WorkHistoryDocument {
                    relative_path: None,
                    entries: Vec::new(),
                    categories: Vec::new(),
                    areas: Vec::new(),
                    warnings: vec![ValidationWarning {
                        code: "state_unavailable".into(),
                        message: error.to_string(),
                        item_id: None,
                    }],
                    preserved_content: None,
                },
                pending_reviews: Vec::new(),
            }
        }),
    }
}

pub fn observe_documents(root: &Path) -> ProjectDocuments {
    ProjectDocuments {
        readme: read_document(root, "README.md", &["README.md"]),
        startup: read_document(root, "STARTUP.md", &["STARTUP.md"]),
        todo: read_document(root, "TODO.md", &["TODO.md"]),
        working_history: read_document(
            root,
            "WORK_HISTORY.md",
            &["WORK_HISTORY.md", "WORKING_HISTORY.md"],
        ),
    }
}

fn read_document(root: &Path, name: &str, aliases: &[&str]) -> DocumentContent {
    let path = match find_document(root, aliases) {
        Ok(Some(path)) => path,
        Ok(None) => {
            return DocumentContent {
                name: name.to_string(),
                relative_path: None,
                status: DocumentStatus::Missing,
                content: None,
                modified_at: None,
                truncated: false,
                error: None,
            };
        }
        Err(error) => return document_error(name, None, error),
    };

    let canonical_root = match root.canonicalize() {
        Ok(path) => path,
        Err(error) => return document_error(name, None, error.to_string()),
    };
    let canonical_path = match path.canonicalize() {
        Ok(path) => path,
        Err(error) => return document_error(name, None, error.to_string()),
    };
    let relative_path = canonical_path
        .strip_prefix(&canonical_root)
        .ok()
        .map(|value| value.to_string_lossy().replace('\\', "/"));
    if !canonical_path.starts_with(&canonical_root) {
        return document_error(
            name,
            relative_path,
            "The document resolves outside the registered project directory".to_string(),
        );
    }

    let metadata = match fs::metadata(&canonical_path) {
        Ok(metadata) if metadata.is_file() => metadata,
        Ok(_) => {
            return document_error(
                name,
                relative_path,
                "The document path is not a file".into(),
            );
        }
        Err(error) => return document_error(name, relative_path, error.to_string()),
    };

    let mut bytes = Vec::new();
    let read_result = File::open(&canonical_path)
        .and_then(|file| file.take(MAX_DOCUMENT_BYTES + 1).read_to_end(&mut bytes));
    if let Err(error) = read_result {
        return document_error(name, relative_path, error.to_string());
    }

    let truncated = bytes.len() as u64 > MAX_DOCUMENT_BYTES;
    if truncated {
        bytes.truncate(MAX_DOCUMENT_BYTES as usize);
    }

    DocumentContent {
        name: name.to_string(),
        relative_path,
        status: DocumentStatus::Available,
        content: Some(String::from_utf8_lossy(&bytes).into_owned()),
        modified_at: metadata.modified().ok().map(DateTime::<Utc>::from),
        truncated,
        error: None,
    }
}

fn find_document(root: &Path, aliases: &[&str]) -> Result<Option<PathBuf>, String> {
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("The registered project directory is inaccessible: {error}"))?;

    for directory in [canonical_root.clone(), canonical_root.join("docs")] {
        if !directory.exists() {
            continue;
        }
        let canonical_directory = directory
            .canonicalize()
            .map_err(|error| format!("Unable to inspect document directory: {error}"))?;
        if !canonical_directory.starts_with(&canonical_root) {
            return Err(
                "The document directory resolves outside the registered project directory"
                    .to_string(),
            );
        }
        if !canonical_directory.is_dir() {
            continue;
        }

        let entries = fs::read_dir(&canonical_directory)
            .map_err(|error| format!("Unable to inspect document directory: {error}"))?
            .map(|entry| {
                entry
                    .map(|value| value.path())
                    .map_err(|error| format!("Unable to inspect document directory: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        for alias in aliases {
            if let Some(path) = entries.iter().find(|path| {
                path.file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case(alias))
            }) {
                return Ok(Some(path.clone()));
            }
        }
    }

    Ok(None)
}

fn document_error(name: &str, relative_path: Option<String>, error: String) -> DocumentContent {
    DocumentContent {
        name: name.to_string(),
        relative_path,
        status: DocumentStatus::Error,
        content: None,
        modified_at: None,
        truncated: false,
        error: Some(error),
    }
}

pub fn observe_git(root: &Path, fetch: &GitFetchSnapshot) -> GitInfo {
    if !root.is_dir() {
        return with_fetch(
            GitInfo::error("The registered project directory is no longer accessible"),
            fetch,
        );
    }

    let repo = match Repository::open(root) {
        Ok(repo) => repo,
        Err(error) if error.code() == ErrorCode::NotFound => {
            return with_fetch(GitInfo::not_repository(), fetch);
        }
        Err(error) => {
            return with_fetch(
                GitInfo::error(format!("Unable to inspect Git repository: {error}")),
                fetch,
            );
        }
    };

    if !repository_is_within_root(&repo, root) {
        return with_fetch(
            GitInfo::error(
                "Git metadata resolves outside the registered project directory and was not read",
            ),
            fetch,
        );
    }

    let branch = match repo.head() {
        Ok(head) if head.is_branch() => head.shorthand().ok().map(str::to_owned),
        Ok(head) => head
            .target()
            .map(|oid| format!("Detached at {}", short_oid(oid))),
        Err(error) if error.code() == ErrorCode::UnbornBranch => Some("No commits yet".into()),
        Err(_) => None,
    };
    let (upstream, ahead, behind, sync_status, sync_message) = upstream_status(&repo);

    let mut status_options = StatusOptions::new();
    status_options
        .include_ignored(false)
        .include_untracked(true)
        .recurse_untracked_dirs(true);
    let statuses = match repo.statuses(Some(&mut status_options)) {
        Ok(statuses) => statuses,
        Err(error) => {
            return GitInfo {
                is_repository: true,
                branch,
                dirty: None,
                recent_commits: Vec::new(),
                last_activity: newest_git_metadata_time(&repo),
                upstream,
                ahead,
                behind,
                sync_status,
                sync_message,
                fetch_status: fetch.status,
                last_successful_fetch: fetch.last_successful_fetch,
                fetch_error: fetch.error.clone(),
                error: Some(format!("Unable to inspect working tree: {error}")),
            };
        }
    };

    let dirty = !statuses.is_empty();
    let recent_commits = recent_commits(&repo);
    let mut last_activity = recent_commits.first().map(|commit| commit.committed_at);
    last_activity = latest(last_activity, newest_git_metadata_time(&repo));
    for entry in statuses.iter() {
        if let Ok(relative) = entry.path() {
            let path = root.join(relative);
            if let Ok(metadata) = fs::symlink_metadata(path)
                && let Ok(modified) = metadata.modified()
            {
                last_activity = latest(last_activity, Some(DateTime::<Utc>::from(modified)));
            }
        }
    }

    GitInfo {
        is_repository: true,
        branch,
        dirty: Some(dirty),
        recent_commits,
        last_activity,
        upstream,
        ahead,
        behind,
        sync_status,
        sync_message,
        fetch_status: fetch.status,
        last_successful_fetch: fetch.last_successful_fetch,
        fetch_error: fetch.error.clone(),
        error: None,
    }
}

fn with_fetch(mut git: GitInfo, fetch: &GitFetchSnapshot) -> GitInfo {
    git.fetch_status = fetch.status;
    git.last_successful_fetch = fetch.last_successful_fetch;
    git.fetch_error = fetch.error.clone();
    git
}

fn upstream_status(
    repo: &Repository,
) -> (
    Option<String>,
    Option<usize>,
    Option<usize>,
    GitSyncStatus,
    Option<String>,
) {
    let head = match repo.head() {
        Ok(head) if head.is_branch() => head,
        Ok(_) => {
            return (
                None,
                None,
                None,
                GitSyncStatus::Unknown,
                Some("Detached HEAD has no upstream branch.".into()),
            );
        }
        Err(error) if error.code() == ErrorCode::UnbornBranch => {
            return (
                None,
                None,
                None,
                GitSyncStatus::Unknown,
                Some("The branch has no commits yet.".into()),
            );
        }
        Err(error) => {
            return (
                None,
                None,
                None,
                GitSyncStatus::Unknown,
                Some(format!("Unable to inspect the current branch: {error}")),
            );
        }
    };
    let branch_name = match head.shorthand() {
        Ok(branch_name) => branch_name,
        Err(error) => {
            return (
                None,
                None,
                None,
                GitSyncStatus::Unknown,
                Some(format!("The current branch name is unavailable: {error}")),
            );
        }
    };
    let branch = match repo.find_branch(branch_name, BranchType::Local) {
        Ok(branch) => branch,
        Err(error) => {
            return (
                None,
                None,
                None,
                GitSyncStatus::Unknown,
                Some(format!("Unable to inspect the current branch: {error}")),
            );
        }
    };
    let upstream = match branch.upstream() {
        Ok(upstream) => upstream,
        Err(error) if error.code() == ErrorCode::NotFound => {
            return (
                None,
                None,
                None,
                GitSyncStatus::Unknown,
                Some("The current branch has no available upstream branch.".into()),
            );
        }
        Err(error) => {
            return (
                None,
                None,
                None,
                GitSyncStatus::Unknown,
                Some(format!("Unable to inspect the upstream branch: {error}")),
            );
        }
    };
    let upstream_name = upstream
        .name()
        .ok()
        .flatten()
        .map(|name| name.trim_start_matches("refs/remotes/").to_owned());
    let Some(local_oid) = branch.get().target() else {
        return (
            upstream_name,
            None,
            None,
            GitSyncStatus::Unknown,
            Some("The current branch target is unavailable.".into()),
        );
    };
    let Some(upstream_oid) = upstream.get().target() else {
        return (
            upstream_name,
            None,
            None,
            GitSyncStatus::Unknown,
            Some("The upstream branch target is unavailable.".into()),
        );
    };
    match repo.graph_ahead_behind(local_oid, upstream_oid) {
        Ok((ahead, behind)) => (
            upstream_name,
            Some(ahead),
            Some(behind),
            classify_sync_status(ahead, behind),
            None,
        ),
        Err(error) => (
            upstream_name,
            None,
            None,
            GitSyncStatus::Unknown,
            Some(format!(
                "Unable to compare with the upstream branch: {error}"
            )),
        ),
    }
}

fn classify_sync_status(ahead: usize, behind: usize) -> GitSyncStatus {
    match (ahead, behind) {
        (0, 0) => GitSyncStatus::Synchronized,
        (_, 0) => GitSyncStatus::Ahead,
        (0, _) => GitSyncStatus::Behind,
        _ => GitSyncStatus::Diverged,
    }
}

fn repository_is_within_root(repo: &Repository, root: &Path) -> bool {
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    let Ok(git_dir) = repo.path().canonicalize() else {
        return false;
    };
    git_dir.starts_with(root)
}

fn recent_commits(repo: &Repository) -> Vec<GitCommit> {
    let Ok(mut walk) = repo.revwalk() else {
        return Vec::new();
    };
    if walk.push_head().is_err() {
        return Vec::new();
    }

    walk.take(MAX_COMMITS)
        .filter_map(Result::ok)
        .filter_map(|oid| repo.find_commit(oid).ok())
        .filter_map(|commit| {
            let committed_at = DateTime::<Utc>::from_timestamp(commit.time().seconds(), 0)?;
            Some(GitCommit {
                id: short_oid(commit.id()),
                summary: commit
                    .summary()
                    .ok()
                    .flatten()
                    .unwrap_or("Untitled commit")
                    .to_string(),
                author: commit
                    .author()
                    .name()
                    .unwrap_or("Unknown author")
                    .to_string(),
                committed_at,
            })
        })
        .collect()
}

fn short_oid(oid: git2::Oid) -> String {
    oid.to_string().chars().take(8).collect()
}

fn newest_git_metadata_time(repo: &Repository) -> Option<DateTime<Utc>> {
    [repo.path().join("HEAD"), repo.path().join("index")]
        .into_iter()
        .filter_map(|path| fs::metadata(path).ok()?.modified().ok())
        .max()
        .map(DateTime::<Utc>::from)
}

fn latest(left: Option<DateTime<Utc>>, right: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{IndexAddOption, Signature};

    #[test]
    fn documents_use_root_then_docs_and_report_missing() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("docs")).unwrap();
        fs::write(temp.path().join("README.md"), "# Root readme").unwrap();
        fs::write(
            temp.path().join("docs/STARTUP.md"),
            "# Startup\n\n```powershell\nnpm run dev\n```",
        )
        .unwrap();
        fs::write(temp.path().join("docs/TODO.md"), "- [ ] Ship").unwrap();

        let documents = observe_documents(temp.path());
        assert_eq!(documents.readme.relative_path.as_deref(), Some("README.md"));
        assert_eq!(documents.readme.content.as_deref(), Some("# Root readme"));
        assert_eq!(
            documents.startup.relative_path.as_deref(),
            Some("docs/STARTUP.md")
        );
        assert!(
            documents
                .startup
                .content
                .as_deref()
                .unwrap()
                .contains("npm run dev")
        );
        assert_eq!(
            documents.todo.relative_path.as_deref(),
            Some("docs/TODO.md")
        );
        assert_eq!(documents.working_history.status, DocumentStatus::Missing);
    }

    #[test]
    fn document_names_and_work_history_aliases_are_case_insensitive() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("docs")).unwrap();
        fs::write(temp.path().join("readme.md"), "# Lowercase readme").unwrap();
        fs::write(temp.path().join("startup.md"), "# Startup").unwrap();
        fs::write(
            temp.path().join("docs/working_history.md"),
            "# Working history",
        )
        .unwrap();

        let documents = observe_documents(temp.path());
        assert_eq!(documents.readme.relative_path.as_deref(), Some("readme.md"));
        assert_eq!(
            documents.startup.relative_path.as_deref(),
            Some("startup.md")
        );
        assert_eq!(
            documents.working_history.relative_path.as_deref(),
            Some("docs/working_history.md")
        );
        assert_eq!(
            documents.working_history.content.as_deref(),
            Some("# Working history")
        );
    }

    #[test]
    fn large_documents_are_truncated_without_failing() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("README.md"),
            vec![b'a'; MAX_DOCUMENT_BYTES as usize + 20],
        )
        .unwrap();

        let document = observe_documents(temp.path()).readme;
        assert_eq!(document.status, DocumentStatus::Available);
        assert!(document.truncated);
        assert_eq!(document.content.unwrap().len(), MAX_DOCUMENT_BYTES as usize);
    }

    #[test]
    fn non_git_directories_are_supported() {
        let temp = tempfile::tempdir().unwrap();
        let git = observe_git(temp.path(), &GitFetchSnapshot::default());
        assert!(!git.is_repository);
        assert!(git.error.is_none());
    }

    #[test]
    fn a_document_directory_is_reported_as_an_error() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("README.md")).unwrap();

        let document = observe_documents(temp.path()).readme;
        assert_eq!(document.status, DocumentStatus::Error);
        assert_eq!(
            document.error.as_deref(),
            Some("The document path is not a file")
        );
    }

    #[test]
    fn git_observation_reports_branch_commits_and_dirty_state() {
        let temp = tempfile::tempdir().unwrap();
        let repo = Repository::init(temp.path()).unwrap();
        fs::write(temp.path().join("README.md"), "first").unwrap();
        commit_all(&repo, "Initial commit");

        let clean = observe_git(temp.path(), &GitFetchSnapshot::default());
        assert!(clean.is_repository);
        assert_eq!(clean.dirty, Some(false));
        assert_eq!(clean.recent_commits[0].summary, "Initial commit");
        assert!(clean.branch.is_some());

        fs::write(temp.path().join("README.md"), "changed").unwrap();
        let dirty = observe_git(temp.path(), &GitFetchSnapshot::default());
        assert_eq!(dirty.dirty, Some(true));
    }

    #[test]
    fn classifies_all_upstream_relationships() {
        assert_eq!(classify_sync_status(0, 0), GitSyncStatus::Synchronized);
        assert_eq!(classify_sync_status(2, 0), GitSyncStatus::Ahead);
        assert_eq!(classify_sync_status(0, 3), GitSyncStatus::Behind);
        assert_eq!(classify_sync_status(1, 1), GitSyncStatus::Diverged);
    }

    #[test]
    fn branch_without_upstream_is_unknown_without_failing_observation() {
        let temp = tempfile::tempdir().unwrap();
        let repo = Repository::init(temp.path()).unwrap();
        fs::write(temp.path().join("README.md"), "first").unwrap();
        commit_all(&repo, "Initial commit");

        let git = observe_git(temp.path(), &GitFetchSnapshot::default());
        assert!(git.is_repository);
        assert_eq!(git.sync_status, GitSyncStatus::Unknown);
        assert!(git.sync_message.unwrap().contains("no available upstream"));
    }

    fn commit_all(repo: &Repository, message: &str) {
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = Signature::now("Projector Test", "test@example.com").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
            .unwrap();
    }
}
