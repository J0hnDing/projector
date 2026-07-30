use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::{NaiveDate, NaiveDateTime};
use serde::Serialize;
use thiserror::Error;

use crate::{
    agent_instructions::PROJECTOR_SECTION,
    models::{
        TodoDocument, TodoItem, TodoPriority, WorkCategory, WorkHistoryDocument, WorkHistoryEntry,
    },
    project_state::{
        atomic_replace, parse_todo_document, parse_work_history_document, render_todo_document,
        render_work_history_document, validate_todos,
    },
};

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("The migration root must be an existing directory named Projects")]
    InvalidRoot,
    #[error("Migration I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Migrated content did not validate: {0}")]
    Validation(String),
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    pub projects_root: PathBuf,
    pub modified_projects: Vec<String>,
    pub migrated_files: Vec<MigratedFile>,
    pub updated_agent_files: Vec<PathBuf>,
    pub warnings: Vec<String>,
    pub skipped_projects: Vec<SkippedProject>,
    pub validation_failures: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigratedFile {
    pub project: String,
    pub path: PathBuf,
    pub backup: PathBuf,
    pub item_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedProject {
    pub project: String,
    pub reason: String,
}

pub fn migrate_projects(projects_root: &Path) -> Result<MigrationReport, MigrationError> {
    let root = projects_root
        .canonicalize()
        .map_err(|_| MigrationError::InvalidRoot)?;
    if !root.is_dir()
        || !root
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("Projects"))
    {
        return Err(MigrationError::InvalidRoot);
    }

    let mut report = MigrationReport {
        projects_root: root.clone(),
        ..MigrationReport::default()
    };
    let mut projects = fs::read_dir(&root)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .collect::<Vec<_>>();
    projects.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());

    for project in projects {
        let project_path = project.path();
        let project_name = project.file_name().to_string_lossy().into_owned();
        let todo_path = find_recognized(&project_path, &["TODO.md"]);
        let history_path =
            find_recognized(&project_path, &["WORK_HISTORY.md", "WORKING_HISTORY.md"]);
        let mut modified = false;

        match todo_path.as_ref() {
            Some(path) => match migrate_todo_file(&project_name, path, &mut report) {
                Ok(changed) => modified |= changed,
                Err(error) => report
                    .validation_failures
                    .push(format!("{project_name}: {}: {error}", path.display())),
            },
            None => report
                .warnings
                .push(format!("{project_name}: no recognizable TODO file found")),
        }
        match history_path.as_ref() {
            Some(path) => match migrate_history_file(&project_name, path, &mut report) {
                Ok(changed) => modified |= changed,
                Err(error) => report
                    .validation_failures
                    .push(format!("{project_name}: {}: {error}", path.display())),
            },
            None => report.warnings.push(format!(
                "{project_name}: no recognizable working-history file found"
            )),
        }

        match update_agents_file(&project_path) {
            Ok(Some((path, backup))) => {
                report.updated_agent_files.push(path);
                report.warnings.push(format!(
                    "{project_name}: AGENTS.md backed up to {}",
                    backup.display()
                ));
                modified = true;
            }
            Ok(None) => {}
            Err(error) => report
                .validation_failures
                .push(format!("{project_name}: AGENTS.md: {error}")),
        }

        if modified {
            report.modified_projects.push(project_name);
        } else if todo_path.is_none() && history_path.is_none() {
            report.skipped_projects.push(SkippedProject {
                project: project_name,
                reason:
                    "No recognizable project-state Markdown files; AGENTS.md was already current."
                        .into(),
            });
        }
    }
    report.modified_projects.sort();
    report.modified_projects.dedup();
    Ok(report)
}

pub fn report_markdown(report: &MigrationReport) -> String {
    fn bullets(values: &[String], empty: &str) -> String {
        if values.is_empty() {
            format!("- {empty}\n")
        } else {
            values.iter().map(|value| format!("- {value}\n")).collect()
        }
    }
    let migrated = report
        .migrated_files
        .iter()
        .map(|file| {
            format!(
                "- {}: `{}` ({} entries; backup `{}`)\n",
                file.project,
                file.path.display(),
                file.item_count,
                file.backup.display()
            )
        })
        .collect::<String>();
    let agents = report
        .updated_agent_files
        .iter()
        .map(|path| format!("- `{}`\n", path.display()))
        .collect::<String>();
    let skipped = report
        .skipped_projects
        .iter()
        .map(|item| format!("- {}: {}\n", item.project, item.reason))
        .collect::<String>();
    format!(
        "# Projector project-state migration report\n\n\
         Projects root: `{}`\n\n\
         ## Modified projects\n\n{}\
         ## Migrated files\n\n{}\
         ## Updated AGENTS.md files\n\n{}\
         ## Warnings\n\n{}\
         ## Skipped projects\n\n{}\
         ## Validation failures\n\n{}",
        report.projects_root.display(),
        bullets(&report.modified_projects, "none"),
        if migrated.is_empty() {
            "- none\n".into()
        } else {
            migrated
        },
        if agents.is_empty() {
            "- none\n".into()
        } else {
            agents
        },
        bullets(&report.warnings, "none"),
        if skipped.is_empty() {
            "- none\n".into()
        } else {
            skipped
        },
        bullets(&report.validation_failures, "none"),
    )
}

fn migrate_todo_file(
    project: &str,
    path: &Path,
    report: &mut MigrationReport,
) -> Result<bool, MigrationError> {
    let original = fs::read_to_string(path)?;
    let (document, warnings) = migrate_todo_content(&original);
    let rendered = render_todo_document(&document);
    if normalize(&original) == rendered {
        return Ok(false);
    }
    let backup = backup(path)?;
    atomic_replace(path, rendered.as_bytes())?;
    let validated = parse_todo_document(&fs::read_to_string(path)?);
    let structural = validate_todos(&validated.items);
    if validated.items.len() != document.items.len()
        || structural.iter().any(|warning| {
            matches!(
                warning.code.as_str(),
                "duplicate_todo_id"
                    | "invalid_todo_id"
                    | "missing_dependency"
                    | "circular_dependency"
            )
        })
    {
        atomic_replace(path, original.as_bytes())?;
        return Err(MigrationError::Validation(format!(
            "{} TODO items did not round-trip",
            document.items.len()
        )));
    }
    report.warnings.extend(
        warnings
            .into_iter()
            .map(|warning| format!("{project}: {warning}")),
    );
    report.migrated_files.push(MigratedFile {
        project: project.into(),
        path: path.to_path_buf(),
        backup,
        item_count: document.items.len(),
    });
    Ok(true)
}

fn migrate_history_file(
    project: &str,
    path: &Path,
    report: &mut MigrationReport,
) -> Result<bool, MigrationError> {
    let original = fs::read_to_string(path)?;
    let (document, warnings) = migrate_history_content(&original);
    let rendered = render_work_history_document(&document);
    if normalize(&original) == rendered {
        return Ok(false);
    }
    let backup = backup(path)?;
    atomic_replace(path, rendered.as_bytes())?;
    let validated = parse_work_history_document(&fs::read_to_string(path)?);
    if validated.entries.len() != document.entries.len() {
        atomic_replace(path, original.as_bytes())?;
        return Err(MigrationError::Validation(format!(
            "{} history entries did not round-trip",
            document.entries.len()
        )));
    }
    report.warnings.extend(
        warnings
            .into_iter()
            .map(|warning| format!("{project}: {warning}")),
    );
    report.migrated_files.push(MigratedFile {
        project: project.into(),
        path: path.to_path_buf(),
        backup,
        item_count: document.entries.len(),
    });
    Ok(true)
}

fn migrate_todo_content(content: &str) -> (TodoDocument, Vec<String>) {
    let parsed = parse_todo_document(content);
    let mut warnings = Vec::new();
    let mut items = parsed.items;

    if items.is_empty() {
        let legacy = legacy_todo_items(content);
        if !legacy.is_empty() {
            items = legacy;
            warnings.push(
                "Legacy TODO sections were converted with low priority and unknown values where fields were absent."
                    .into(),
            );
        }
    } else {
        warnings.push(
            "Existing structured TODO fields were normalized to the Projector syntax.".into(),
        );
    }

    for item in &mut items {
        if item.rationale.to_lowercase().starts_with("migration note:") {
            item.rationale = "unknown".into();
        }
        if item
            .acceptance_criteria
            .to_lowercase()
            .starts_with("migration note:")
        {
            item.acceptance_criteria = "unknown".into();
        }
        let area = slug_area(&item.area);
        if area != item.area {
            warnings.push(format!(
                "{} area `{}` normalized to `{area}`",
                item.id, item.area
            ));
            item.area = area;
        }
    }
    (
        TodoDocument {
            relative_path: None,
            items,
            warnings: Vec::new(),
            preserved_content: None,
        },
        warnings,
    )
}

fn legacy_todo_items(content: &str) -> Vec<TodoItem> {
    let mut titles = Vec::new();
    for line in normalize(content).lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed
            .strip_prefix("- [ ]")
            .or_else(|| {
                let rest = trimmed.strip_prefix("## ")?;
                let (_, title) = rest.split_once(". ")?;
                Some(title)
            })
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            titles.push(title.to_string());
        }
    }
    titles
        .into_iter()
        .enumerate()
        .map(|(index, title)| TodoItem {
            id: format!("TODO-{:03}", index + 1),
            category: infer_category(&title),
            title,
            priority: TodoPriority::Low,
            area: "unknown".into(),
            dependencies: Vec::new(),
            rationale: "unknown".into(),
            acceptance_criteria: "unknown".into(),
        })
        .collect()
}

fn migrate_history_content(content: &str) -> (WorkHistoryDocument, Vec<String>) {
    let parsed = parse_work_history_document(content);
    if !parsed.entries.is_empty() {
        let mut entries = parsed.entries;
        for entry in &mut entries {
            if entry
                .limitations
                .to_lowercase()
                .starts_with("migration note:")
            {
                entry.limitations = "none".into();
            }
            if entry.summary.to_lowercase().starts_with("migration note:") {
                entry.summary = "unknown".into();
            }
            entry.summary = entry
                .summary
                .replace("explicit migration notes", "explicit unknown values");
            entry.limitations = entry
                .limitations
                .replace("explicit migration notes", "explicit unknown values");
        }
        return (
            WorkHistoryDocument {
                relative_path: None,
                entries,
                categories: Vec::new(),
                areas: Vec::new(),
                warnings: Vec::new(),
                preserved_content: None,
            },
            Vec::new(),
        );
    }

    let normalized = normalize(content);
    let blocks = legacy_heading_blocks(&normalized);
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    for (heading, body) in blocks {
        let Some((occurred_at, title, missing_time)) = legacy_history_heading(&heading) else {
            continue;
        };
        if missing_time {
            warnings.push(format!(
                "`{title}` had no time; migration used 00:00 local time"
            ));
        }
        let (summary, limitations, limitations_missing) = legacy_history_body(&body);
        if limitations_missing {
            warnings.push(format!(
                "`{title}` had no explicit limitations; migration used none"
            ));
        }
        entries.push(WorkHistoryEntry {
            occurred_at,
            title: title.clone(),
            category: infer_category(&title),
            area: "unknown".into(),
            summary,
            limitations,
        });
    }
    if entries.is_empty() {
        warnings.push("No recognizable dated working-history entries were found.".into());
    } else {
        warnings.push(
            "Legacy history categories were inferred from titles and areas were set to unknown."
                .into(),
        );
    }
    (
        WorkHistoryDocument {
            relative_path: None,
            entries,
            categories: Vec::new(),
            areas: Vec::new(),
            warnings: Vec::new(),
            preserved_content: None,
        },
        warnings,
    )
}

fn legacy_heading_blocks(content: &str) -> Vec<(String, String)> {
    let mut blocks = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut body = Vec::new();
    for line in content.lines() {
        if line.starts_with("## ") {
            if let Some(heading) = current_heading.take() {
                blocks.push((heading, body.join("\n")));
                body.clear();
            }
            current_heading = Some(line.to_string());
        } else if current_heading.is_some() {
            body.push(line.to_string());
        }
    }
    if let Some(heading) = current_heading {
        blocks.push((heading, body.join("\n")));
    }
    blocks
}

fn legacy_history_heading(heading: &str) -> Option<(NaiveDateTime, String, bool)> {
    let value = heading.strip_prefix("## ")?.trim();
    if value.len() < 10 {
        return None;
    }
    if value.len() >= 16 {
        if let Ok(date_time) = NaiveDateTime::parse_from_str(&value[..16], "%Y-%m-%d %H:%M") {
            let title = value[16..]
                .trim_start_matches([' ', '-', '—'])
                .trim()
                .to_string();
            return (!title.is_empty()).then_some((date_time, title, false));
        }
    }
    let date = NaiveDate::parse_from_str(&value[..10], "%Y-%m-%d").ok()?;
    let title = value[10..]
        .trim_start_matches([' ', '-', '—'])
        .trim()
        .to_string();
    (!title.is_empty()).then_some((date.and_hms_opt(0, 0, 0)?, title, true))
}

fn legacy_history_body(body: &str) -> (String, String, bool) {
    let mut summary = Vec::new();
    let mut limitations = None;
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(value) = field_value(trimmed, "Summary") {
            summary.push(value.to_string());
        } else if let Some(value) = field_value(trimmed, "Limitations/Future implementations") {
            limitations = Some(value.to_string());
        } else if !trimmed.is_empty() {
            summary.push(line.to_string());
        }
    }
    let summary = if summary.is_empty() {
        "unknown".into()
    } else {
        summary.join("\n").trim().to_string()
    };
    let missing = limitations.is_none();
    (
        summary,
        limitations.unwrap_or_else(|| "none".into()),
        missing,
    )
}

fn field_value<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let line = line.strip_prefix('-').unwrap_or(line).trim();
    let prefix = format!("{name}:");
    line.strip_prefix(&prefix).map(str::trim)
}

fn infer_category(title: &str) -> WorkCategory {
    let title = title.to_lowercase();
    if ["test", "verification"]
        .iter()
        .any(|word| title.contains(word))
    {
        WorkCategory::Test
    } else if ["documentation", "docs", "guide"]
        .iter()
        .any(|word| title.contains(word))
    {
        WorkCategory::Documentation
    } else if ["audit", "research", "investigation"]
        .iter()
        .any(|word| title.contains(word))
    {
        WorkCategory::Research
    } else if ["decision", "policy"]
        .iter()
        .any(|word| title.contains(word))
    {
        WorkCategory::Others
    } else if [
        "fix",
        "failure",
        "error",
        "recovery",
        "reliability",
        "hardening",
    ]
    .iter()
    .any(|word| title.contains(word))
    {
        WorkCategory::Bugfix
    } else if ["refactor", "decomposition", "cleanup", "rename", "remove"]
        .iter()
        .any(|word| title.contains(word))
    {
        WorkCategory::Refactor
    } else {
        WorkCategory::Feature
    }
}

fn slug_area(area: &str) -> String {
    let mut slug = String::new();
    let mut hyphen = false;
    for character in area.trim().to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            hyphen = false;
        } else if !hyphen && !slug.is_empty() {
            slug.push('-');
            hyphen = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn update_agents_file(project: &Path) -> Result<Option<(PathBuf, PathBuf)>, MigrationError> {
    let path = project.join("AGENTS.md");
    let original = if path.exists() {
        fs::read_to_string(&path)?
    } else {
        String::new()
    };
    let updated = replace_section(&normalize(&original), "Projector", PROJECTOR_SECTION);
    if normalize(&original) == updated {
        return Ok(None);
    }
    let backup = backup(&path)?;
    atomic_replace(&path, updated.as_bytes())?;
    Ok(Some((path, backup)))
}

fn replace_section(content: &str, title: &str, section: &str) -> String {
    let heading = format!("## {title}");
    let mut output = Vec::new();
    let mut lines = content.lines().peekable();
    let mut replaced = false;
    while let Some(line) = lines.next() {
        if line.trim().eq_ignore_ascii_case(&heading) {
            if !output.is_empty() && output.last().is_some_and(|value: &&str| !value.is_empty()) {
                output.push("");
            }
            output.extend(section.trim().lines());
            replaced = true;
            while lines.peek().is_some_and(|next| !next.starts_with("## ")) {
                lines.next();
            }
        } else {
            output.push(line);
        }
    }
    let mut result = output.join("\n").trim_end().to_string();
    if !replaced {
        if !result.is_empty() {
            result.push_str("\n\n");
        }
        result.push_str(section.trim());
    }
    result.push('\n');
    result
}

fn find_recognized(project: &Path, aliases: &[&str]) -> Option<PathBuf> {
    for directory in [project.to_path_buf(), project.join("docs")] {
        let entries = fs::read_dir(directory).ok()?;
        for entry in entries.filter_map(Result::ok) {
            if entry.file_type().is_ok_and(|kind| kind.is_file())
                && aliases.iter().any(|alias| {
                    entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| name.eq_ignore_ascii_case(alias))
                })
            {
                return Some(entry.path());
            }
        }
    }
    None
}

fn backup(path: &Path) -> Result<PathBuf, std::io::Error> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project-state");
    let backup = path.with_file_name(format!("{file_name}.projector-backup"));
    if !backup.exists() {
        if path.exists() {
            fs::copy(path, &backup)?;
        } else {
            atomic_replace(&backup, b"")?;
        }
    }
    Ok(backup)
}

fn normalize(content: &str) -> String {
    let mut value = content.replace("\r\n", "\n").replace('\r', "\n");
    if !value.is_empty() && !value.ends_with('\n') {
        value.push('\n');
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_is_idempotent_without_redundant_migration_notes() {
        let temp = tempfile::tempdir().unwrap();
        let projects = temp.path().join("Projects");
        let project = projects.join("Example");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("TODO.md"),
            "# TODO\n\n- [ ] Keep this original task",
        )
        .unwrap();
        fs::write(
            project.join("WORK_HISTORY.md"),
            "# History\n\n## 2026-07-20 — Initial work\n\n- Implemented the first version.",
        )
        .unwrap();

        let first = migrate_projects(&projects).unwrap();
        assert_eq!(first.migrated_files.len(), 2);
        let todo_after_first = fs::read_to_string(project.join("TODO.md")).unwrap();
        let history_after_first = fs::read_to_string(project.join("WORK_HISTORY.md")).unwrap();
        assert!(todo_after_first.contains("Keep this original task"));
        assert!(todo_after_first.contains("- Rationale: unknown"));
        assert!(todo_after_first.contains("- Category:"));
        assert!(!todo_after_first.contains("- Status:"));
        assert!(!todo_after_first.contains("Migration note"));
        assert!(!todo_after_first.contains("## Migration Notes"));
        assert!(!history_after_first.contains("Migration note"));
        assert!(!history_after_first.contains("## Migration Notes"));
        assert!(!history_after_first.contains("- Related TODOs:"));
        assert!(project.join("TODO.md.projector-backup").exists());

        let second = migrate_projects(&projects).unwrap();
        assert!(second.migrated_files.is_empty());
        assert_eq!(
            fs::read_to_string(project.join("TODO.md")).unwrap(),
            todo_after_first
        );
        assert_eq!(
            fs::read_to_string(project.join("WORK_HISTORY.md")).unwrap(),
            history_after_first
        );
    }

    #[test]
    fn agents_section_is_replaced_without_touching_other_instructions() {
        let content =
            "# AGENTS\n\n## Existing\n\nKeep.\n\n## Projector\n\nOld.\n\n## Later\n\nAlso keep.\n";
        let updated = replace_section(content, "Projector", PROJECTOR_SECTION);
        assert!(updated.contains("## Existing\n\nKeep."));
        assert!(updated.contains("POST /projects/{projectId}/todos/{todoId}/complete"));
        assert!(updated.contains(
            "Use `POST /projects/{projectId}/todos/{todoId}/complete` when that TODO is finished."
        ));
        assert!(!updated.contains("TODO status is derived"));
        assert!(updated.contains("## Later\n\nAlso keep."));
        assert!(!updated.contains("\nOld.\n"));
    }
}
