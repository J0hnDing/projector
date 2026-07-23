use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use git2::{ErrorCode, Repository, StatusOptions};

use crate::models::{
    DocumentContent, DocumentStatus, GitCommit, GitInfo, ProjectDetail, ProjectDocuments,
    ProjectSummary, RegistryEntry,
};

const MAX_DOCUMENT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_COMMITS: usize = 25;

pub fn summarize(entry: &RegistryEntry) -> ProjectSummary {
    ProjectSummary {
        id: entry.id,
        name: entry.name.clone(),
        path: entry.path.clone(),
        registered_at: entry.registered_at,
        last_opened: entry.last_opened,
        git: observe_git(&entry.path),
    }
}

pub fn detail(entry: &RegistryEntry) -> ProjectDetail {
    ProjectDetail {
        project: summarize(entry),
        documents: observe_documents(&entry.path),
    }
}

pub fn observe_documents(root: &Path) -> ProjectDocuments {
    ProjectDocuments {
        readme: read_document(root, "README.md", &["README.md"]),
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

pub fn observe_git(root: &Path) -> GitInfo {
    if !root.is_dir() {
        return GitInfo::error("The registered project directory is no longer accessible");
    }

    let repo = match Repository::open(root) {
        Ok(repo) => repo,
        Err(error) if error.code() == ErrorCode::NotFound => return GitInfo::not_repository(),
        Err(error) => return GitInfo::error(format!("Unable to inspect Git repository: {error}")),
    };

    if !repository_is_within_root(&repo, root) {
        return GitInfo::error(
            "Git metadata resolves outside the registered project directory and was not read",
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
        error: None,
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
        fs::write(temp.path().join("docs/TODO.md"), "- [ ] Ship").unwrap();

        let documents = observe_documents(temp.path());
        assert_eq!(documents.readme.relative_path.as_deref(), Some("README.md"));
        assert_eq!(documents.readme.content.as_deref(), Some("# Root readme"));
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
        fs::write(
            temp.path().join("docs/working_history.md"),
            "# Working history",
        )
        .unwrap();

        let documents = observe_documents(temp.path());
        assert_eq!(documents.readme.relative_path.as_deref(), Some("readme.md"));
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
        let git = observe_git(temp.path());
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

        let clean = observe_git(temp.path());
        assert!(clean.is_repository);
        assert_eq!(clean.dirty, Some(false));
        assert_eq!(clean.recent_commits[0].summary, "Initial commit");
        assert!(clean.branch.is_some());

        fs::write(temp.path().join("README.md"), "changed").unwrap();
        let dirty = observe_git(temp.path());
        assert_eq!(dirty.dirty, Some(true));
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
