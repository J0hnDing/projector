use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    agent_instructions::{PROJECTOR_SECTION, migrate_agents, new_project_agents},
    models::RegistryEntry,
    project_state::atomic_replace,
};

const DEFAULT_SETTINGS: &str = include_str!("../resources/subagent-defaults.toml");
const SETTINGS_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkerAgentSettings {
    #[serde(rename = "fileName", alias = "file_name")]
    pub file_name: String,
    pub name: String,
    pub description: String,
    pub model: String,
    #[serde(rename = "modelReasoningEffort", alias = "model_reasoning_effort")]
    pub model_reasoning_effort: ReasoningEffort,
    #[serde(rename = "sandboxMode", alias = "sandbox_mode")]
    pub sandbox_mode: String,
    #[serde(rename = "developerInstructions", alias = "developer_instructions")]
    pub developer_instructions: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SubagentSettings {
    pub version: u8,
    #[serde(
        rename = "customSection",
        alias = "custom_section",
        default = "default_custom_section"
    )]
    pub custom_section: String,
    #[serde(
        rename = "projectorSection",
        alias = "projector_section",
        default = "default_projector_section"
    )]
    pub projector_section: String,
    #[serde(rename = "subagentsSection", alias = "subagents_section")]
    pub subagents_section: String,
    #[serde(rename = "workerLow", alias = "worker_low")]
    pub worker_low: WorkerAgentSettings,
    #[serde(rename = "workerMedium", alias = "worker_medium")]
    pub worker_medium: WorkerAgentSettings,
    #[serde(rename = "workerHigh", alias = "worker_high")]
    pub worker_high: WorkerAgentSettings,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedSubagentFile {
    pub path: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SettingsMigrationResult {
    pub project_id: uuid::Uuid,
    pub project_name: String,
    pub updated_files: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Error)]
pub enum SubagentSettingsError {
    #[error("Unable to read or write subagent settings: {0}")]
    Io(#[from] std::io::Error),
    #[error("The bundled subagent defaults are invalid: {0}")]
    Defaults(#[from] toml::de::Error),
    #[error("The saved subagent settings are invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("The subagent settings are invalid: {0}")]
    Validation(String),
    #[error("Unable to render a custom agent file: {0}")]
    Render(#[from] toml::ser::Error),
}

pub struct SubagentSettingsStore {
    file_path: PathBuf,
    defaults: SubagentSettings,
    settings: SubagentSettings,
}

impl SubagentSettingsStore {
    pub fn load(file_path: PathBuf) -> Result<Self, SubagentSettingsError> {
        let defaults = bundled_defaults()?;
        let settings = if file_path.exists() {
            let settings = serde_json::from_slice(&fs::read(&file_path)?)?;
            validate_settings(&settings, &defaults)?;
            settings
        } else {
            defaults.clone()
        };
        Ok(Self {
            file_path,
            defaults,
            settings,
        })
    }

    pub fn settings(&self) -> &SubagentSettings {
        &self.settings
    }

    pub fn save(
        &mut self,
        settings: SubagentSettings,
    ) -> Result<SubagentSettings, SubagentSettingsError> {
        validate_settings(&settings, &self.defaults)?;
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.file_path, serde_json::to_vec_pretty(&settings)?)?;
        self.settings = settings;
        Ok(self.settings.clone())
    }

    pub fn reset(&mut self) -> Result<SubagentSettings, SubagentSettingsError> {
        if self.file_path.exists() {
            fs::remove_file(&self.file_path)?;
        }
        self.settings = self.defaults.clone();
        Ok(self.settings.clone())
    }

    pub fn preview(
        &self,
        settings: &SubagentSettings,
    ) -> Result<Vec<GeneratedSubagentFile>, SubagentSettingsError> {
        validate_settings(settings, &self.defaults)?;
        settings.generated_files("Example Project")
    }
}

impl SubagentSettings {
    pub fn generated_files(
        &self,
        project_name: &str,
    ) -> Result<Vec<GeneratedSubagentFile>, SubagentSettingsError> {
        let mut files = vec![GeneratedSubagentFile {
            path: "AGENTS.md".into(),
            content: new_project_agents(
                project_name,
                &self.custom_section,
                &self.subagents_section,
                &self.projector_section,
            ),
        }];
        for worker in [&self.worker_low, &self.worker_medium, &self.worker_high] {
            files.push(GeneratedSubagentFile {
                path: format!(".codex/agents/{}", worker.file_name),
                content: render_worker(worker)?,
            });
        }
        Ok(files)
    }
}

pub fn migrate_project_settings(
    entry: &RegistryEntry,
    settings: &SubagentSettings,
) -> Result<SettingsMigrationResult, SubagentSettingsError> {
    let root = entry.path.canonicalize()?;
    if root != entry.path || !root.is_dir() {
        return Err(SubagentSettingsError::Validation(format!(
            "{} no longer resolves to its registered project directory",
            entry.name
        )));
    }

    let generated = settings.generated_files(&entry.name)?;
    let mut updates = Vec::new();
    for file in generated {
        let path = root.join(&file.path);
        validate_managed_path(&root, &path)?;
        let original = match fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let content = if file.path == "AGENTS.md" {
            let existing = original
                .as_ref()
                .map(|bytes| String::from_utf8(bytes.clone()))
                .transpose()
                .map_err(|_| {
                    SubagentSettingsError::Validation(
                        "AGENTS.md is not valid UTF-8 and was not changed".into(),
                    )
                })?;
            migrate_agents(
                existing.as_deref(),
                &entry.name,
                &settings.custom_section,
                &settings.subagents_section,
                &settings.projector_section,
            )
        } else {
            file.content
        };
        if original.as_deref() != Some(content.as_bytes()) {
            updates.push((file.path, path, original, content.into_bytes()));
        }
    }

    for (_, path, original, _) in &updates {
        if original.is_some() {
            let backup = backup_path(path);
            validate_managed_path(&root, &backup)?;
            if !backup.exists() {
                fs::copy(path, backup)?;
            }
        }
    }

    for (_, path, _, _) in &updates {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut applied: Vec<(String, PathBuf, Option<Vec<u8>>)> = Vec::new();
    for (relative, path, original, content) in &updates {
        if let Err(error) = atomic_replace(path, content) {
            for (_, applied_path, applied_original) in applied.into_iter().rev() {
                match applied_original {
                    Some(bytes) => {
                        let _ = atomic_replace(&applied_path, &bytes);
                    }
                    None => {
                        let _ = fs::remove_file(&applied_path);
                    }
                }
            }
            return Err(error.into());
        }
        applied.push((relative.clone(), path.clone(), original.clone()));
    }

    Ok(SettingsMigrationResult {
        project_id: entry.id,
        project_name: entry.name.clone(),
        updated_files: updates.into_iter().map(|(relative, ..)| relative).collect(),
        error: None,
    })
}

fn validate_managed_path(root: &Path, path: &Path) -> Result<(), SubagentSettingsError> {
    let mut ancestor = path;
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or_else(|| {
            SubagentSettingsError::Validation("managed path has no existing parent".into())
        })?;
    }
    let canonical = ancestor.canonicalize()?;
    if !canonical.starts_with(root) {
        return Err(SubagentSettingsError::Validation(
            "a managed settings path resolves outside the registered project directory".into(),
        ));
    }
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("settings");
    path.with_file_name(format!("{name}.projector-backup"))
}

fn default_projector_section() -> String {
    PROJECTOR_SECTION.to_string()
}

fn default_custom_section() -> String {
    "## Custom instructions".to_string()
}

pub fn bundled_defaults() -> Result<SubagentSettings, SubagentSettingsError> {
    let defaults: SubagentSettings = toml::from_str(DEFAULT_SETTINGS)?;
    validate_settings(&defaults, &defaults)?;
    Ok(defaults)
}

fn validate_settings(
    settings: &SubagentSettings,
    defaults: &SubagentSettings,
) -> Result<(), SubagentSettingsError> {
    if settings.version != SETTINGS_VERSION {
        return Err(SubagentSettingsError::Validation(format!(
            "unsupported settings version {}",
            settings.version
        )));
    }
    let custom_section = settings.custom_section.trim();
    if !custom_section.starts_with("## Custom instructions") {
        return Err(SubagentSettingsError::Validation(
            "the AGENTS.md custom guidance must begin with `## Custom instructions`".into(),
        ));
    }
    if custom_section.len() > 32_000 {
        return Err(SubagentSettingsError::Validation(
            "the AGENTS.md custom guidance is too large".into(),
        ));
    }
    let section = settings.subagents_section.trim();
    if !section.starts_with("## Subagents") {
        return Err(SubagentSettingsError::Validation(
            "the AGENTS.md guidance must begin with `## Subagents`".into(),
        ));
    }
    if section.len() > 32_000 {
        return Err(SubagentSettingsError::Validation(
            "the AGENTS.md subagent guidance is too large".into(),
        ));
    }
    let projector_section = settings.projector_section.trim();
    if !projector_section.starts_with("## Projector") {
        return Err(SubagentSettingsError::Validation(
            "the AGENTS.md Projector guidance must begin with `## Projector`".into(),
        ));
    }
    if projector_section.len() > 32_000 {
        return Err(SubagentSettingsError::Validation(
            "the AGENTS.md Projector guidance is too large".into(),
        ));
    }
    validate_worker("worker_low", &settings.worker_low, &defaults.worker_low)?;
    validate_worker(
        "worker_medium",
        &settings.worker_medium,
        &defaults.worker_medium,
    )?;
    validate_worker("worker_high", &settings.worker_high, &defaults.worker_high)?;
    Ok(())
}

fn validate_worker(
    role: &str,
    worker: &WorkerAgentSettings,
    defaults: &WorkerAgentSettings,
) -> Result<(), SubagentSettingsError> {
    let (expected_file_name, expected_name) = match role {
        "worker_low" => ("worker-low.toml", "worker_low"),
        "worker_medium" => ("worker-medium.toml", "worker_medium"),
        "worker_high" => ("worker-high.toml", "worker_high"),
        _ => {
            return Err(SubagentSettingsError::Validation(format!(
                "unknown worker role {role}"
            )));
        }
    };
    if defaults.file_name != expected_file_name
        || defaults.name != expected_name
        || defaults.sandbox_mode != "workspace-write"
    {
        return Err(SubagentSettingsError::Validation(format!(
            "the bundled {role} identity is invalid"
        )));
    }
    if worker.file_name != defaults.file_name
        || worker.name != defaults.name
        || worker.sandbox_mode != defaults.sandbox_mode
    {
        return Err(SubagentSettingsError::Validation(format!(
            "{role} identity, filename, and sandbox mode cannot be changed"
        )));
    }
    for (field, value, limit) in [
        ("description", worker.description.trim(), 2_000),
        ("model", worker.model.trim(), 200),
        (
            "developer instructions",
            worker.developer_instructions.trim(),
            32_000,
        ),
    ] {
        if value.is_empty() {
            return Err(SubagentSettingsError::Validation(format!(
                "{role} {field} cannot be empty"
            )));
        }
        if value.len() > limit {
            return Err(SubagentSettingsError::Validation(format!(
                "{role} {field} is too large"
            )));
        }
    }
    if worker.model.chars().any(char::is_whitespace) {
        return Err(SubagentSettingsError::Validation(format!(
            "{role} model must be a single model identifier"
        )));
    }
    Ok(())
}

#[derive(Serialize)]
struct CustomAgentFile<'a> {
    name: &'a str,
    description: &'a str,
    model: &'a str,
    model_reasoning_effort: ReasoningEffort,
    sandbox_mode: &'a str,
    developer_instructions: &'a str,
}

fn render_worker(worker: &WorkerAgentSettings) -> Result<String, toml::ser::Error> {
    let rendered = toml::to_string_pretty(&CustomAgentFile {
        name: &worker.name,
        description: &worker.description,
        model: &worker.model,
        model_reasoning_effort: worker.model_reasoning_effort,
        sandbox_mode: &worker.sandbox_mode,
        developer_instructions: &worker.developer_instructions,
    })?;
    Ok(format!("{rendered}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn bundled_defaults_define_the_requested_worker_tiers() {
        let settings = bundled_defaults().unwrap();

        assert!(
            settings
                .custom_section
                .starts_with("## Custom instructions")
        );
        assert_eq!(
            settings.projector_section.replace("\r\n", "\n").trim(),
            PROJECTOR_SECTION.replace("\r\n", "\n").trim()
        );
        assert!(settings.subagents_section.contains(
            "Use the built-in `explorer` for independent, read-only codebase investigation."
        ));
        assert!(settings.subagents_section.contains(
            "For work that creates or changes an interface, define the expected inputs,"
        ));
        assert_eq!(settings.worker_low.model, "gpt-5.6-luna");
        assert_eq!(
            settings.worker_low.model_reasoning_effort,
            ReasoningEffort::Medium
        );
        assert_eq!(settings.worker_medium.model, "gpt-5.6-luna");
        assert_eq!(
            settings.worker_medium.model_reasoning_effort,
            ReasoningEffort::Max
        );
        assert_eq!(settings.worker_high.model, "gpt-5.6-sol");
        assert_eq!(
            settings.worker_high.model_reasoning_effort,
            ReasoningEffort::High
        );
    }

    #[test]
    fn saved_version_one_settings_without_new_sections_are_upgraded() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("subagent-settings.json");
        let mut value = serde_json::to_value(bundled_defaults().unwrap()).unwrap();
        value.as_object_mut().unwrap().remove("projectorSection");
        value.as_object_mut().unwrap().remove("customSection");
        fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();

        let store = SubagentSettingsStore::load(path).unwrap();

        assert_eq!(store.settings().projector_section, PROJECTOR_SECTION);
        assert_eq!(store.settings().custom_section, "## Custom instructions");
    }

    #[test]
    fn migration_updates_only_managed_files_and_is_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("Example");
        fs::create_dir(&project).unwrap();
        fs::write(
            project.join("AGENTS.md"),
            "# Existing\n\n## Keep\n\nUntouched.\n\n## Subagents\n\nOld.\n\n## Projector\n\nOld.\n",
        )
        .unwrap();
        fs::create_dir_all(project.join(".codex/agents")).unwrap();
        fs::write(project.join(".codex/agents/worker-low.toml"), "old").unwrap();
        let entry = RegistryEntry {
            id: Uuid::new_v4(),
            name: "Example".into(),
            path: project.canonicalize().unwrap(),
            registered_at: Utc::now(),
            last_opened: None,
        };
        let mut settings = bundled_defaults().unwrap();
        settings.custom_section = "## Custom instructions\n\nCustom project guidance.".into();
        settings.projector_section = "## Projector\n\nCustom API guidance.".into();
        settings.subagents_section = "## Subagents\n\nCustom worker guidance.".into();

        let first = migrate_project_settings(&entry, &settings).unwrap();
        assert_eq!(first.updated_files.len(), 4);
        let agents = fs::read_to_string(project.join("AGENTS.md")).unwrap();
        assert!(agents.contains("## Keep\n\nUntouched."));
        assert!(agents.contains("Custom project guidance."));
        assert!(agents.contains("Custom API guidance."));
        assert!(agents.contains("Custom worker guidance."));
        assert!(project.join("AGENTS.md.projector-backup").exists());
        assert!(
            project
                .join(".codex/agents/worker-low.toml.projector-backup")
                .exists()
        );
        assert!(
            !project
                .join(".codex/agents/worker-medium.toml.projector-backup")
                .exists()
        );

        let second = migrate_project_settings(&entry, &settings).unwrap();
        assert!(second.updated_files.is_empty());
    }

    #[test]
    fn migration_rejects_non_utf8_agents_without_changing_it() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("Binary");
        fs::create_dir(&project).unwrap();
        let original = vec![0xff, 0xfe, 0xfd];
        fs::write(project.join("AGENTS.md"), &original).unwrap();
        let entry = RegistryEntry {
            id: Uuid::new_v4(),
            name: "Binary".into(),
            path: project.canonicalize().unwrap(),
            registered_at: Utc::now(),
            last_opened: None,
        };

        let error = migrate_project_settings(&entry, &bundled_defaults().unwrap()).unwrap_err();

        assert!(error.to_string().contains("not valid UTF-8"));
        assert_eq!(fs::read(project.join("AGENTS.md")).unwrap(), original);
        assert!(!project.join("AGENTS.md.projector-backup").exists());
    }

    #[test]
    fn settings_are_saved_reloaded_and_reset_to_bundled_defaults() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("subagent-settings.json");
        let mut store = SubagentSettingsStore::load(path.clone()).unwrap();
        let mut changed = store.settings().clone();
        changed.worker_low.description = "A user-edited description.".into();

        store.save(changed.clone()).unwrap();
        assert_eq!(
            SubagentSettingsStore::load(path.clone())
                .unwrap()
                .settings(),
            &changed
        );

        let reset = store.reset().unwrap();
        assert_eq!(reset, bundled_defaults().unwrap());
        assert!(!path.exists());
    }

    #[test]
    fn immutable_worker_identity_and_invalid_guidance_are_rejected() {
        let defaults = bundled_defaults().unwrap();
        let mut changed = defaults.clone();
        changed.worker_low.name = "renamed".into();
        assert!(validate_settings(&changed, &defaults).is_err());

        let mut changed = defaults.clone();
        changed.subagents_section = "Use workers.".into();
        assert!(validate_settings(&changed, &defaults).is_err());
    }

    #[test]
    fn generated_files_are_valid_custom_agent_toml() {
        let files = bundled_defaults()
            .unwrap()
            .generated_files("Example")
            .unwrap();

        assert_eq!(files.len(), 4);
        assert!(files[0].content.contains("## Subagents"));
        assert!(files[0].content.contains("## Projector"));
        for file in &files[1..] {
            let parsed: toml::Value = toml::from_str(&file.content).unwrap();
            assert!(parsed.get("name").is_some());
            assert!(parsed.get("developer_instructions").is_some());
        }
    }
}
