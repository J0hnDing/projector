use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryEntry {
    pub id: Uuid,
    pub name: String,
    pub path: PathBuf,
    pub registered_at: DateTime<Utc>,
    pub last_opened: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryData {
    pub version: u8,
    pub projects: Vec<RegistryEntry>,
}

impl Default for RegistryData {
    fn default() -> Self {
        Self {
            version: 1,
            projects: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: Uuid,
    pub name: String,
    pub path: PathBuf,
    pub registered_at: DateTime<Utc>,
    pub last_opened: Option<DateTime<Utc>>,
    pub git: GitInfo,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetail {
    pub project: ProjectSummary,
    pub documents: ProjectDocuments,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDocuments {
    pub readme: DocumentContent,
    pub todo: DocumentContent,
    pub working_history: DocumentContent,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentContent {
    pub name: String,
    pub relative_path: Option<String>,
    pub status: DocumentStatus,
    pub content: Option<String>,
    pub modified_at: Option<DateTime<Utc>>,
    pub truncated: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum DocumentStatus {
    Available,
    Missing,
    Error,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitInfo {
    pub is_repository: bool,
    pub branch: Option<String>,
    pub dirty: Option<bool>,
    pub recent_commits: Vec<GitCommit>,
    pub last_activity: Option<DateTime<Utc>>,
    pub upstream: Option<String>,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
    pub sync_status: GitSyncStatus,
    pub sync_message: Option<String>,
    pub fetch_status: GitFetchStatus,
    pub last_successful_fetch: Option<DateTime<Utc>>,
    pub fetch_error: Option<String>,
    pub error: Option<String>,
}

impl GitInfo {
    pub fn not_repository() -> Self {
        Self {
            is_repository: false,
            branch: None,
            dirty: None,
            recent_commits: Vec::new(),
            last_activity: None,
            upstream: None,
            ahead: None,
            behind: None,
            sync_status: GitSyncStatus::Unknown,
            sync_message: None,
            fetch_status: GitFetchStatus::Idle,
            last_successful_fetch: None,
            fetch_error: None,
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            error: Some(message.into()),
            ..Self::not_repository()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GitSyncStatus {
    Ahead,
    Behind,
    Diverged,
    Synchronized,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GitFetchStatus {
    #[default]
    Idle,
    Fetching,
    Succeeded,
    NoRemote,
    AuthenticationFailed,
    Offline,
    TimedOut,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommit {
    pub id: String,
    pub summary: String,
    pub author: String,
    pub committed_at: DateTime<Utc>,
}
