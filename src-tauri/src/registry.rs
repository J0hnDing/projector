use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Utc;
use git2::{Config, Reference, Repository, RepositoryInitOptions};
use thiserror::Error;
use uuid::Uuid;

use crate::models::{RegistryData, RegistryEntry};
use crate::subagent_settings::{SubagentSettings, SubagentSettingsError};

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("Project directory does not exist or cannot be accessed: {0}")]
    InvalidDirectory(String),
    #[error("Unable to read or write the project registry: {0}")]
    Io(#[from] std::io::Error),
    #[error("The project registry is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Unsupported project registry version: {0}")]
    UnsupportedVersion(u8),
    #[error("Project name is invalid: {0}")]
    InvalidProjectName(String),
    #[error("A file or folder already exists at the new project path: {0}")]
    ProjectAlreadyExists(String),
    #[error("Unable to initialize the Git repository: {0}")]
    Git(#[from] git2::Error),
    #[error("Unable to generate the project's subagent configuration: {0}")]
    SubagentSettings(#[from] SubagentSettingsError),
}

pub struct RegistryStore {
    file_path: PathBuf,
    data: RegistryData,
}

impl RegistryStore {
    pub fn load(file_path: PathBuf) -> Result<Self, RegistryError> {
        let data = if file_path.exists() {
            serde_json::from_slice::<RegistryData>(&fs::read(&file_path)?)?
        } else {
            RegistryData::default()
        };

        if data.version != 1 {
            return Err(RegistryError::UnsupportedVersion(data.version));
        }

        Ok(Self { file_path, data })
    }

    pub fn entries(&self) -> &[RegistryEntry] {
        &self.data.projects
    }

    pub fn find(&self, id: Uuid) -> Option<&RegistryEntry> {
        self.data.projects.iter().find(|entry| entry.id == id)
    }

    pub fn register(&mut self, raw_path: &Path) -> Result<RegistryEntry, RegistryError> {
        let path = raw_path.canonicalize().map_err(|_| {
            RegistryError::InvalidDirectory(raw_path.to_string_lossy().into_owned())
        })?;
        if !path.is_dir() {
            return Err(RegistryError::InvalidDirectory(
                raw_path.to_string_lossy().into_owned(),
            ));
        }

        if let Some(existing) = self
            .data
            .projects
            .iter()
            .find(|entry| path_key(&entry.path) == path_key(&path))
        {
            return Ok(existing.clone());
        }

        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let entry = RegistryEntry {
            id: Uuid::new_v4(),
            name,
            path,
            registered_at: Utc::now(),
            last_opened: None,
        };

        let mut next = self.data.clone();
        next.projects.push(entry.clone());
        self.persist(&next)?;
        self.data = next;
        Ok(entry)
    }

    pub fn create_and_register(
        &mut self,
        raw_parent: &Path,
        raw_name: &str,
        initialize_git: bool,
        subagent_settings: &SubagentSettings,
    ) -> Result<RegistryEntry, RegistryError> {
        let parent = raw_parent.canonicalize().map_err(|_| {
            RegistryError::InvalidDirectory(raw_parent.to_string_lossy().into_owned())
        })?;
        if !parent.is_dir() {
            return Err(RegistryError::InvalidDirectory(
                raw_parent.to_string_lossy().into_owned(),
            ));
        }
        let name = validate_project_name(raw_name)?;
        let project = parent.join(&name);
        if project.exists() {
            return Err(RegistryError::ProjectAlreadyExists(
                project.to_string_lossy().into_owned(),
            ));
        }

        fs::create_dir(&project)?;
        let result = (|| {
            write_new_file(
                &project.join("README.md"),
                format!("# {name}\n\nDescribe the project here.\n").as_bytes(),
            )?;
            for file in subagent_settings.generated_files(&name)? {
                let path = project.join(file.path);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                write_new_file(&path, file.content.as_bytes())?;
            }
            write_new_file(&project.join("TODO.md"), b"")?;
            write_new_file(&project.join("WORK_HISTORY.md"), b"")?;
            if initialize_git {
                initialize_repository(&project)?;
            }
            self.register(&project)
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&project);
        }
        result
    }

    pub fn remove(&mut self, id: Uuid) -> Result<Option<RegistryEntry>, RegistryError> {
        let Some(index) = self.data.projects.iter().position(|entry| entry.id == id) else {
            return Ok(None);
        };

        let mut next = self.data.clone();
        let removed = next.projects.remove(index);
        self.persist(&next)?;
        self.data = next;
        Ok(Some(removed))
    }

    pub fn touch(&mut self, id: Uuid) -> Result<RegistryEntry, RegistryError> {
        let mut next = self.data.clone();
        let Some(entry) = next.projects.iter_mut().find(|entry| entry.id == id) else {
            return Err(RegistryError::InvalidDirectory(format!(
                "Unknown project id {id}"
            )));
        };
        entry.last_opened = Some(Utc::now());
        let updated = entry.clone();
        self.persist(&next)?;
        self.data = next;
        Ok(updated)
    }

    fn persist(&self, data: &RegistryData) -> Result<(), RegistryError> {
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.file_path, serde_json::to_vec_pretty(data)?)?;
        Ok(())
    }
}

fn initialize_repository(project: &Path) -> Result<(), git2::Error> {
    let configured_branch = Config::open_default()
        .ok()
        .and_then(|config| config.get_string("init.defaultBranch").ok())
        .map(|branch| branch.trim().to_string())
        .filter(|branch| {
            !branch.is_empty() && Reference::is_valid_name(&format!("refs/heads/{branch}"))
        });
    let initial_branch = configured_branch.as_deref().unwrap_or("main");
    let mut options = RepositoryInitOptions::new();
    options.initial_head(initial_branch);
    Repository::init_opts(project, &options).map(|_| ())
}

fn validate_project_name(raw_name: &str) -> Result<String, RegistryError> {
    let name = raw_name.trim();
    if name.is_empty()
        || matches!(name, "." | "..")
        || name.ends_with('.')
        || name.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
        })
    {
        return Err(RegistryError::InvalidProjectName(
            "Use a folder name without path separators, control characters, or Windows-reserved punctuation".into(),
        ));
    }
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
    {
        return Err(RegistryError::InvalidProjectName(
            "That name is reserved by Windows".into(),
        ));
    }
    Ok(name.to_string())
}

fn write_new_file(path: &Path, content: &[u8]) -> Result<(), std::io::Error> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(content)?;
    file.sync_all()
}

fn path_key(path: &Path) -> String {
    let key = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        key.to_lowercase()
    } else {
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subagent_settings::{ReasoningEffort, bundled_defaults};

    fn subagent_defaults() -> SubagentSettings {
        bundled_defaults().unwrap()
    }

    #[test]
    fn registration_is_persisted_and_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let registry_path = temp.path().join("app-data/registered-projects.json");
        let project = temp.path().join("example");
        fs::create_dir(&project).unwrap();

        let mut store = RegistryStore::load(registry_path.clone()).unwrap();
        let first = store.register(&project).unwrap();
        let duplicate = store.register(&project).unwrap();
        assert_eq!(first.id, duplicate.id);
        assert_eq!(store.entries().len(), 1);

        let reloaded = RegistryStore::load(registry_path).unwrap();
        assert_eq!(reloaded.entries().len(), 1);
        assert_eq!(reloaded.entries()[0].path, project.canonicalize().unwrap());
        assert!(!project.join(".codex").exists());
    }

    #[test]
    fn removal_never_deletes_the_project_directory() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("keep-me");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("README.md"), "still here").unwrap();
        let mut store = RegistryStore::load(temp.path().join("registry.json")).unwrap();
        let entry = store.register(&project).unwrap();

        assert!(store.remove(entry.id).unwrap().is_some());
        assert!(project.join("README.md").exists());
        assert!(store.entries().is_empty());
    }

    #[test]
    fn invalid_and_file_paths_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = RegistryStore::load(temp.path().join("registry.json")).unwrap();
        let file = temp.path().join("file.txt");
        fs::write(&file, "x").unwrap();

        assert!(store.register(&file).is_err());
        assert!(store.register(&temp.path().join("missing")).is_err());
        assert!(store.entries().is_empty());
    }

    #[test]
    fn project_creation_initializes_files_and_registers_the_new_directory() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = RegistryStore::load(temp.path().join("registry.json")).unwrap();

        let entry = store
            .create_and_register(temp.path(), "New Project", true, &subagent_defaults())
            .unwrap();

        assert_eq!(entry.name, "New Project");
        assert_eq!(store.entries().len(), 1);
        assert_eq!(
            fs::read_to_string(entry.path.join("README.md")).unwrap(),
            "# New Project\n\nDescribe the project here.\n"
        );
        assert_eq!(fs::read_to_string(entry.path.join("TODO.md")).unwrap(), "");
        assert_eq!(
            fs::read_to_string(entry.path.join("WORK_HISTORY.md")).unwrap(),
            ""
        );
        let instructions = fs::read_to_string(entry.path.join("AGENTS.md")).unwrap();
        assert!(instructions.contains("POST /projects/{projectId}/todos"));
        assert!(instructions.contains("category"));
        assert!(instructions.contains(
            "Use `POST /projects/{projectId}/todos/{todoId}/complete` when that TODO is finished."
        ));
        assert!(!instructions.contains("TODO status is derived"));
        assert!(instructions.contains("## Subagents"));
        assert!(instructions.find("## Subagents") < instructions.find("## Projector"));
        let low: toml::Value = toml::from_str(
            &fs::read_to_string(entry.path.join(".codex/agents/worker-low.toml")).unwrap(),
        )
        .unwrap();
        assert_eq!(low["name"].as_str(), Some("worker_low"));
        assert_eq!(low["model"].as_str(), Some("gpt-5.6-luna"));
        assert_eq!(low["model_reasoning_effort"].as_str(), Some("medium"));
        let medium: toml::Value = toml::from_str(
            &fs::read_to_string(entry.path.join(".codex/agents/worker-medium.toml")).unwrap(),
        )
        .unwrap();
        assert_eq!(medium["model"].as_str(), Some("gpt-5.6-luna"));
        assert_eq!(medium["model_reasoning_effort"].as_str(), Some("max"));
        let high = &subagent_defaults().worker_high;
        assert_eq!(high.model_reasoning_effort, ReasoningEffort::High);
        assert!(entry.path.join(".codex/agents/worker-high.toml").is_file());
        let repository = Repository::open(&entry.path).unwrap();
        match repository.head() {
            Err(error) => assert_eq!(error.code(), git2::ErrorCode::UnbornBranch),
            Ok(_) => panic!("new repository unexpectedly has a commit"),
        }
    }

    #[test]
    fn project_creation_can_leave_git_uninitialized() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = RegistryStore::load(temp.path().join("registry.json")).unwrap();

        let entry = store
            .create_and_register(temp.path(), "Plain Project", false, &subagent_defaults())
            .unwrap();

        assert!(entry.path.join("README.md").is_file());
        assert!(!entry.path.join(".git").exists());
        assert!(Repository::open(&entry.path).is_err());
    }

    #[test]
    fn project_creation_snapshots_user_edited_subagent_settings() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = RegistryStore::load(temp.path().join("registry.json")).unwrap();
        let mut settings = subagent_defaults();
        settings
            .subagents_section
            .push_str("\n\nCustom project guidance.");
        settings.worker_low.model = "custom-worker-model".into();
        settings.worker_low.description = "A customized low worker.".into();

        let entry = store
            .create_and_register(temp.path(), "Customized", false, &settings)
            .unwrap();

        assert!(
            fs::read_to_string(entry.path.join("AGENTS.md"))
                .unwrap()
                .contains("Custom project guidance.")
        );
        let low: toml::Value = toml::from_str(
            &fs::read_to_string(entry.path.join(".codex/agents/worker-low.toml")).unwrap(),
        )
        .unwrap();
        assert_eq!(low["model"].as_str(), Some("custom-worker-model"));
        assert_eq!(
            low["description"].as_str(),
            Some("A customized low worker.")
        );
    }

    #[test]
    fn failed_registration_rolls_back_initialized_project_directory() {
        let temp = tempfile::tempdir().unwrap();
        let registry_path = temp.path().join("registry.json");
        let mut store = RegistryStore::load(registry_path.clone()).unwrap();
        fs::create_dir(&registry_path).unwrap();

        assert!(
            store
                .create_and_register(temp.path(), "Rolled Back", true, &subagent_defaults())
                .is_err()
        );

        assert!(!temp.path().join("Rolled Back").exists());
        assert!(store.entries().is_empty());
    }

    #[test]
    fn project_creation_rejects_unsafe_or_existing_names_without_modifying_them() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("Existing")).unwrap();
        let mut store = RegistryStore::load(temp.path().join("registry.json")).unwrap();

        assert!(
            store
                .create_and_register(temp.path(), "../outside", true, &subagent_defaults())
                .is_err()
        );
        assert!(
            store
                .create_and_register(temp.path(), "CON", true, &subagent_defaults())
                .is_err()
        );
        assert!(
            store
                .create_and_register(temp.path(), "Existing", true, &subagent_defaults())
                .is_err()
        );
        assert!(temp.path().join("Existing").is_dir());
        assert!(!temp.path().join("outside").exists());
        assert!(store.entries().is_empty());
    }
}
