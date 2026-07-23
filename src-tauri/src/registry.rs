use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use thiserror::Error;
use uuid::Uuid;

use crate::models::{RegistryData, RegistryEntry};

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
}
