use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use chrono::{Local, NaiveDateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use thiserror::Error;
use uuid::Uuid;

use crate::models::{
    AddTodoInput, AddWorkHistoryInput, CompleteTodoInput, CompleteTodoResult, CompletionProposal,
    ProjectState, ProposalKind, TodoDocument, TodoItem, TodoPriority, ValidationWarning,
    WorkCategory, WorkHistoryDocument, WorkHistoryEntry,
};

const TODO_FILE: &str = "TODO.md";
const HISTORY_FILE: &str = "WORK_HISTORY.md";
const TODO_ALIASES: &[&str] = &["TODO.md"];
const HISTORY_ALIASES: &[&str] = &["WORK_HISTORY.md", "WORKING_HISTORY.md"];

#[derive(Debug, Error)]
pub enum ProjectStateError {
    #[error("The registered project directory is inaccessible: {0}")]
    InvalidRoot(String),
    #[error("Project state is invalid: {0}")]
    Validation(String),
    #[error("Unable to update project state: {0}")]
    Io(#[from] io::Error),
    #[error("Pending review storage is invalid: {0}")]
    ProposalStorage(String),
}

pub struct ProjectStateService {
    write_lock: Mutex<()>,
    proposal_file: Option<PathBuf>,
    proposals: Mutex<CompletionProposalData>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompletionProposalData {
    version: u8,
    proposals: Vec<CompletionProposal>,
}

impl Default for CompletionProposalData {
    fn default() -> Self {
        Self {
            version: 1,
            proposals: Vec::new(),
        }
    }
}

impl Default for ProjectStateService {
    fn default() -> Self {
        Self {
            write_lock: Mutex::new(()),
            proposal_file: None,
            proposals: Mutex::new(CompletionProposalData::default()),
        }
    }
}

impl ProjectStateService {
    pub fn load(proposal_file: PathBuf) -> Result<Self, ProjectStateError> {
        let proposals = if proposal_file.exists() {
            serde_json::from_slice::<CompletionProposalData>(&fs::read(&proposal_file)?)
                .map_err(|error| ProjectStateError::ProposalStorage(error.to_string()))?
        } else {
            CompletionProposalData::default()
        };
        if proposals.version != 1 {
            return Err(ProjectStateError::ProposalStorage(format!(
                "Unsupported pending review storage version {}",
                proposals.version
            )));
        }
        Ok(Self {
            write_lock: Mutex::new(()),
            proposal_file: Some(proposal_file),
            proposals: Mutex::new(proposals),
        })
    }

    pub fn inspect(&self, root: &Path) -> Result<ProjectState, ProjectStateError> {
        inspect_project_state(root)
    }

    pub fn pending_reviews(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<CompletionProposal>, ProjectStateError> {
        let proposals = self.proposals.lock().map_err(|_| {
            ProjectStateError::ProposalStorage("The proposal store is unavailable".into())
        })?;
        let mut pending: Vec<_> = proposals
            .proposals
            .iter()
            .filter(|proposal| proposal.project_id == project_id)
            .cloned()
            .collect();
        pending.sort_by(|left, right| right.requested_at.cmp(&left.requested_at));
        Ok(pending)
    }

    pub fn add_todo(
        &self,
        root: &Path,
        input: AddTodoInput,
    ) -> Result<TodoItem, ProjectStateError> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| ProjectStateError::Validation("The state writer is unavailable".into()))?;
        validate_nonempty("title", &input.title)?;
        validate_nonempty("area", &input.area)?;
        validate_nonempty("rationale", &input.rationale)?;
        validate_nonempty("acceptance criteria", &input.acceptance_criteria)?;
        validate_todo_ids(&input.dependencies, "dependencies")?;

        let root = canonical_root(root)?;
        let path = writable_document_path(&root, TODO_FILE, TODO_ALIASES)?;
        let original = read_optional(&path)?;
        let mut document = parse_todo_document(original.as_deref().unwrap_or(""));
        ensure_mutable_todo_document(&document)?;

        let known: HashSet<_> = document.items.iter().map(|item| item.id.as_str()).collect();
        for dependency in &input.dependencies {
            if !known.contains(dependency.as_str()) {
                return Err(ProjectStateError::Validation(format!(
                    "Dependency {dependency} does not exist in this project"
                )));
            }
        }

        let item = TodoItem {
            id: next_todo_id(&document.items),
            title: input.title.trim().to_string(),
            priority: input.priority,
            category: input.category,
            area: input.area.trim().to_string(),
            dependencies: deduplicate(input.dependencies),
            rationale: input.rationale.trim().to_string(),
            acceptance_criteria: input.acceptance_criteria.trim().to_string(),
        };
        document.items.push(item.clone());
        document.warnings = validate_todos(&document.items);
        ensure_mutable_todo_document(&document)?;
        atomic_replace(&path, render_todo_document(&document).as_bytes())?;
        Ok(item)
    }

    pub fn add_work_history(
        &self,
        project_id: Uuid,
        root: &Path,
        input: AddWorkHistoryInput,
    ) -> Result<CompletionProposal, ProjectStateError> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| ProjectStateError::Validation("The state writer is unavailable".into()))?;
        validate_nonempty("title", &input.title)?;
        validate_nonempty("area", &input.area)?;
        validate_nonempty("summary", &input.summary)?;
        validate_nonempty("limitations", &input.limitations)?;

        let root = canonical_root(root)?;
        let history_path = writable_document_path(&root, HISTORY_FILE, HISTORY_ALIASES)?;
        let history_original = read_optional(&history_path)?;
        let history = parse_work_history_document(history_original.as_deref().unwrap_or(""));
        ensure_mutable_history_document(&history)?;

        let proposal = CompletionProposal {
            id: Uuid::new_v4(),
            project_id,
            requested_at: Utc::now(),
            kind: ProposalKind::WorkHistory,
            todo: None,
            proposed_entry: WorkHistoryEntry {
                occurred_at: local_minute(),
                title: input.title.trim().to_string(),
                category: input.category,
                area: input.area.trim().to_string(),
                summary: input.summary.trim().to_string(),
                limitations: input.limitations.trim().to_string(),
            },
        };
        let mut proposals = self.proposals.lock().map_err(|_| {
            ProjectStateError::ProposalStorage("The proposal store is unavailable".into())
        })?;
        let mut next = proposals.clone();
        next.proposals.push(proposal.clone());
        self.persist_proposals(&next)?;
        *proposals = next;
        Ok(proposal)
    }

    pub fn complete_todo(
        &self,
        project_id: Uuid,
        root: &Path,
        todo_id: &str,
        input: CompleteTodoInput,
    ) -> Result<CompletionProposal, ProjectStateError> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| ProjectStateError::Validation("The state writer is unavailable".into()))?;
        validate_todo_id(todo_id)?;
        validate_nonempty("summary", &input.summary)?;
        validate_nonempty("limitations", &input.limitations)?;

        let root = canonical_root(root)?;
        let todo_path = writable_document_path(&root, TODO_FILE, TODO_ALIASES)?;
        let todo_original = read_optional(&todo_path)?;
        let todos = parse_todo_document(todo_original.as_deref().unwrap_or(""));
        ensure_mutable_todo_document(&todos)?;

        let Some(todo) = todos.items.iter().find(|item| item.id == todo_id).cloned() else {
            return Err(ProjectStateError::Validation(format!(
                "TODO {} does not exist",
                todo_id
            )));
        };
        for dependency in &todo.dependencies {
            if !todos.items.iter().any(|item| &item.id == dependency) {
                return Err(ProjectStateError::Validation(format!(
                    "TODO {} references missing dependency {dependency}",
                    todo.id
                )));
            }
        }

        let mut proposals = self.proposals.lock().map_err(|_| {
            ProjectStateError::ProposalStorage("The proposal store is unavailable".into())
        })?;
        if proposals.proposals.iter().any(|proposal| {
            proposal.project_id == project_id
                && proposal
                    .todo
                    .as_ref()
                    .is_some_and(|todo| todo.id == todo_id)
        }) {
            return Err(ProjectStateError::Validation(format!(
                "TODO {todo_id} already has a pending completion proposal"
            )));
        }

        let proposal = CompletionProposal {
            id: Uuid::new_v4(),
            project_id,
            requested_at: Utc::now(),
            kind: ProposalKind::TodoCompletion,
            todo: Some(todo.clone()),
            proposed_entry: WorkHistoryEntry {
                occurred_at: local_minute(),
                title: todo.title,
                category: todo.category,
                area: todo.area,
                summary: input.summary.trim().to_string(),
                limitations: input.limitations.trim().to_string(),
            },
        };
        let mut next = proposals.clone();
        next.proposals.push(proposal.clone());
        self.persist_proposals(&next)?;
        *proposals = next;
        Ok(proposal)
    }

    pub fn approve_completion(
        &self,
        project_id: Uuid,
        root: &Path,
        proposal_id: Uuid,
    ) -> Result<CompleteTodoResult, ProjectStateError> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| ProjectStateError::Validation("The state writer is unavailable".into()))?;
        let mut proposals = self.proposals.lock().map_err(|_| {
            ProjectStateError::ProposalStorage("The proposal store is unavailable".into())
        })?;
        let Some(proposal_index) = proposals
            .proposals
            .iter()
            .position(|proposal| proposal.id == proposal_id && proposal.project_id == project_id)
        else {
            return Err(ProjectStateError::Validation(format!(
                "Pending review proposal {proposal_id} does not exist for this project"
            )));
        };
        let proposal = proposals.proposals[proposal_index].clone();

        let root = canonical_root(root)?;
        let history_path = writable_document_path(&root, HISTORY_FILE, HISTORY_ALIASES)?;
        let history_original = read_optional(&history_path)?;
        let mut history = parse_work_history_document(history_original.as_deref().unwrap_or(""));
        ensure_mutable_history_document(&history)?;
        history.entries.push(proposal.proposed_entry.clone());
        let history_bytes = render_work_history_document(&history).into_bytes();

        let (completed_todo, todo_rollback) = match proposal.kind {
            ProposalKind::TodoCompletion => {
                let proposed_todo = proposal.todo.as_ref().ok_or_else(|| {
                    ProjectStateError::ProposalStorage(format!(
                        "TODO completion proposal {} has no TODO snapshot",
                        proposal.id
                    ))
                })?;
                let todo_path = writable_document_path(&root, TODO_FILE, TODO_ALIASES)?;
                let todo_original = read_optional(&todo_path)?;
                let mut todos = parse_todo_document(todo_original.as_deref().unwrap_or(""));
                ensure_mutable_todo_document(&todos)?;
                let Some(index) = todos
                    .items
                    .iter()
                    .position(|item| item.id == proposed_todo.id)
                else {
                    return Err(ProjectStateError::Validation(format!(
                        "TODO {} no longer exists; reject the stale proposal",
                        proposed_todo.id
                    )));
                };
                if todos.items[index] != *proposed_todo {
                    return Err(ProjectStateError::Validation(format!(
                        "TODO {} changed after this proposal was submitted; reject it and request completion again",
                        proposed_todo.id
                    )));
                }

                let completed = todos.items.remove(index);
                for item in &mut todos.items {
                    item.dependencies
                        .retain(|dependency| dependency != &completed.id);
                }
                todos.warnings = validate_todos(&todos.items);
                ensure_mutable_todo_document(&todos)?;
                let todo_bytes = render_todo_document(&todos).into_bytes();
                replace_pair_with_rollback(
                    (&todo_path, &todo_bytes, todo_original.as_deref()),
                    (&history_path, &history_bytes, history_original.as_deref()),
                )?;
                (Some(completed), Some((todo_path, todo_original)))
            }
            ProposalKind::WorkHistory => {
                if proposal.todo.is_some() {
                    return Err(ProjectStateError::ProposalStorage(format!(
                        "Working-history proposal {} unexpectedly contains a TODO snapshot",
                        proposal.id
                    )));
                }
                atomic_replace(&history_path, &history_bytes)?;
                (None, None)
            }
        };

        let mut next = proposals.clone();
        next.proposals.remove(proposal_index);
        if let Err(error) = self.persist_proposals(&next) {
            if let Some((todo_path, todo_original)) = &todo_rollback {
                restore_pair(
                    (todo_path, todo_original.as_deref()),
                    (&history_path, history_original.as_deref()),
                )?;
            } else {
                restore_file(&history_path, history_original.as_deref())?;
            }
            return Err(error);
        }
        *proposals = next;

        Ok(CompleteTodoResult {
            completed_todo,
            history_entry: proposal.proposed_entry,
        })
    }

    pub fn reject_completion(
        &self,
        project_id: Uuid,
        proposal_id: Uuid,
    ) -> Result<CompletionProposal, ProjectStateError> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| ProjectStateError::Validation("The state writer is unavailable".into()))?;
        let mut proposals = self.proposals.lock().map_err(|_| {
            ProjectStateError::ProposalStorage("The proposal store is unavailable".into())
        })?;
        let Some(index) = proposals
            .proposals
            .iter()
            .position(|proposal| proposal.id == proposal_id && proposal.project_id == project_id)
        else {
            return Err(ProjectStateError::Validation(format!(
                "Pending review proposal {proposal_id} does not exist for this project"
            )));
        };
        let mut next = proposals.clone();
        let rejected = next.proposals.remove(index);
        self.persist_proposals(&next)?;
        *proposals = next;
        Ok(rejected)
    }

    fn persist_proposals(
        &self,
        proposals: &CompletionProposalData,
    ) -> Result<(), ProjectStateError> {
        let Some(path) = &self.proposal_file else {
            return Ok(());
        };
        let bytes = serde_json::to_vec_pretty(proposals)
            .map_err(|error| ProjectStateError::ProposalStorage(error.to_string()))?;
        atomic_replace(path, &bytes)?;
        Ok(())
    }
}

pub fn inspect_project_state(root: &Path) -> Result<ProjectState, ProjectStateError> {
    let root = canonical_root(root)?;
    let todo_path = find_document(&root, TODO_ALIASES)?;
    let history_path = find_document(&root, HISTORY_ALIASES)?;

    let mut todos = parse_todo_document(
        todo_path
            .as_ref()
            .map(fs::read_to_string)
            .transpose()?
            .as_deref()
            .unwrap_or(""),
    );
    todos.relative_path = todo_path
        .as_ref()
        .and_then(|path| relative_path(&root, path));

    let mut history = parse_work_history_document(
        history_path
            .as_ref()
            .map(fs::read_to_string)
            .transpose()?
            .as_deref()
            .unwrap_or(""),
    );
    history.relative_path = history_path
        .as_ref()
        .and_then(|path| relative_path(&root, path));
    history.entries.sort_by(|left, right| {
        right
            .occurred_at
            .cmp(&left.occurred_at)
            .then_with(|| left.title.cmp(&right.title))
    });
    history.categories = history
        .entries
        .iter()
        .map(|entry| entry.category)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    history.areas = history
        .entries
        .iter()
        .map(|entry| entry.area.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    todos.items.sort_by(|left, right| {
        left.priority
            .rank()
            .cmp(&right.priority.rank())
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(ProjectState {
        todos,
        working_history: history,
        pending_reviews: Vec::new(),
    })
}

pub fn parse_todo_document(content: &str) -> TodoDocument {
    let normalized = normalize_newlines(content);
    let mut items = Vec::new();
    let mut warnings = Vec::new();
    let mut preserved = Vec::new();
    let blocks = split_heading_blocks(&normalized);

    for block in blocks {
        let Some((id, title, standard_heading)) = parse_todo_heading(block.heading) else {
            if let Some(content) = preserved_block(&block) {
                preserved.push(content);
            }
            continue;
        };
        if !standard_heading {
            warnings.push(warning(
                "nonstandard_todo_heading",
                "TODO heading should use `## TODO-NNN: Title`.",
                Some(&id),
            ));
        }

        let mut fields = HashMap::new();
        let mut acceptance_lines = Vec::new();
        let mut in_acceptance = false;
        for line in block.body.lines() {
            if let Some((key, value)) = parse_field_line(line) {
                if key == "acceptance criteria" {
                    in_acceptance = true;
                    if !value.is_empty() {
                        acceptance_lines.push(value.to_string());
                    }
                } else if !in_acceptance {
                    fields.insert(key, value.to_string());
                } else {
                    acceptance_lines.push(line.to_string());
                }
            } else if in_acceptance {
                acceptance_lines.push(line.to_string());
            } else if !line.trim().is_empty() {
                warnings.push(warning(
                    "unrecognized_todo_content",
                    "Unrecognized content was preserved with this TODO.",
                    Some(&id),
                ));
                acceptance_lines.push(format!("<!-- Projector preserved: {line} -->"));
            }
        }

        let priority = match fields
            .get("priority")
            .map(|value| value.trim().to_lowercase())
        {
            Some(value) if value == "critical" => Some(TodoPriority::Critical),
            Some(value) if value == "high" => Some(TodoPriority::High),
            Some(value) if value == "medium" => Some(TodoPriority::Medium),
            Some(value) if value == "low" => Some(TodoPriority::Low),
            Some(value) => {
                warnings.push(warning(
                    "invalid_priority",
                    &format!(
                        "Unsupported priority `{value}`; expected critical, high, medium, or low."
                    ),
                    Some(&id),
                ));
                None
            }
            None => {
                warnings.push(warning(
                    "missing_priority",
                    "Missing Priority field.",
                    Some(&id),
                ));
                None
            }
        };
        let category = match fields.get("category") {
            Some(value) => {
                let parsed = parse_category(Some(value));
                if parsed.is_none() {
                    warnings.push(warning(
                        "missing_or_invalid_category",
                        "Invalid Category field.",
                        Some(&id),
                    ));
                }
                parsed
            }
            None => {
                warnings.push(warning(
                    "missing_category_defaulted",
                    "Missing Category field was interpreted as others.",
                    Some(&id),
                ));
                Some(WorkCategory::Others)
            }
        };
        let area = required_field(&fields, "area", &id, &mut warnings);
        let rationale = required_field(&fields, "rationale", &id, &mut warnings);
        let dependencies = match fields.get("dependencies") {
            Some(value) => parse_id_list(value, &id, "dependencies", &mut warnings),
            None => {
                warnings.push(warning(
                    "missing_dependencies",
                    "Missing Dependencies field.",
                    Some(&id),
                ));
                Vec::new()
            }
        };
        let acceptance_criteria = acceptance_lines.join("\n").trim().to_string();
        if acceptance_criteria.is_empty() {
            warnings.push(warning(
                "missing_acceptance_criteria",
                "Missing Acceptance Criteria field or body.",
                Some(&id),
            ));
        }

        if let (Some(priority), Some(category), Some(area), Some(rationale)) =
            (priority, category, area, rationale)
            && !title.trim().is_empty()
            && !acceptance_criteria.is_empty()
        {
            items.push(TodoItem {
                id,
                title: title.trim().to_string(),
                priority,
                category,
                area,
                dependencies,
                rationale,
                acceptance_criteria,
            });
        } else {
            preserved.push(block.raw.trim().to_string());
        }
    }
    warnings.extend(validate_todos(&items));
    if !preserved.is_empty() {
        warnings.push(warning(
            "preserved_unrecognized_content",
            "Unrecognized source content is preserved for review.",
            None,
        ));
    }
    TodoDocument {
        relative_path: None,
        items,
        warnings: deduplicate_warnings(warnings),
        preserved_content: nonempty_join(preserved),
    }
}

pub fn validate_todos(items: &[TodoItem]) -> Vec<ValidationWarning> {
    let mut warnings = Vec::new();
    let mut ids = HashSet::new();
    for item in items {
        if !is_todo_id(&item.id) {
            warnings.push(warning(
                "invalid_todo_id",
                "TODO ID must use the form TODO-NNN.",
                Some(&item.id),
            ));
        }
        if !ids.insert(item.id.as_str()) {
            warnings.push(warning(
                "duplicate_todo_id",
                "TODO ID is duplicated.",
                Some(&item.id),
            ));
        }
    }
    for item in items {
        for dependency in &item.dependencies {
            if !ids.contains(dependency.as_str()) {
                warnings.push(warning(
                    "missing_dependency",
                    &format!("Dependency {dependency} does not exist in this project."),
                    Some(&item.id),
                ));
            }
            if dependency == &item.id {
                warnings.push(warning(
                    "circular_dependency",
                    "TODO depends on itself.",
                    Some(&item.id),
                ));
            }
        }
    }

    let graph: HashMap<_, _> = items
        .iter()
        .map(|item| (item.id.as_str(), item.dependencies.as_slice()))
        .collect();
    for item in items {
        let mut visiting = HashSet::new();
        if has_cycle(item.id.as_str(), item.id.as_str(), &graph, &mut visiting) {
            warnings.push(warning(
                "circular_dependency",
                "TODO participates in a circular dependency.",
                Some(&item.id),
            ));
        }
    }
    deduplicate_warnings(warnings)
}

pub fn render_todo_document(document: &TodoDocument) -> String {
    let mut items = document.items.clone();
    items.sort_by(|left, right| left.id.cmp(&right.id));
    let mut output = String::new();
    for item in items {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&format!("## {}: {}\n\n", item.id, item.title.trim()));
        output.push_str(&format!("- Priority: {}\n", todo_priority(item.priority)));
        output.push_str(&format!("- Category: {}\n", work_category(item.category)));
        output.push_str(&format!("- Area: {}\n", item.area.trim()));
        output.push_str(&format!(
            "- Dependencies: {}\n",
            if item.dependencies.is_empty() {
                "none".to_string()
            } else {
                item.dependencies.join(", ")
            }
        ));
        output.push_str(&format!("- Rationale: {}\n\n", item.rationale.trim()));
        output.push_str("-Acceptance Criteria:\n");
        output.push_str(item.acceptance_criteria.trim());
        output.push('\n');
    }
    if let Some(content) = document
        .preserved_content
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str("## Preserved Unrecognized Content\n\n");
        output.push_str(
            "The following source content was not recognized as structured project state and is preserved verbatim.\n\n",
        );
        output.push_str(content.trim());
        output.push('\n');
    }
    output
}

pub fn parse_work_history_document(content: &str) -> WorkHistoryDocument {
    let normalized = normalize_newlines(content);
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    let mut preserved = Vec::new();
    for block in split_heading_blocks(&normalized) {
        let Some((occurred_at, title, standard_heading)) = parse_history_heading(block.heading)
        else {
            if let Some(content) = preserved_block(&block) {
                preserved.push(content);
            }
            continue;
        };
        if !standard_heading {
            warnings.push(warning(
                "nonstandard_history_heading",
                "History heading should use `## YYYY-MM-DD HH:MM — Title`.",
                None,
            ));
        }
        let mut fields = HashMap::new();
        let mut section = "";
        let mut summary = Vec::new();
        let mut limitations = Vec::new();
        for line in block.body.lines() {
            let trimmed = line.trim();
            if trimmed.eq_ignore_ascii_case("### Summary") {
                section = "summary";
                continue;
            }
            if trimmed.eq_ignore_ascii_case("### Limitations") {
                section = "limitations";
                continue;
            }
            if let Some((key, value)) = parse_field_line(line) {
                if matches!(key.as_str(), "category" | "area") && section.is_empty() {
                    fields.insert(key, value.to_string());
                    continue;
                }
                if key == "related todos" && section.is_empty() {
                    continue;
                }
            }
            match section {
                "summary" => summary.push(line.to_string()),
                "limitations" => limitations.push(line.to_string()),
                _ if !trimmed.is_empty() => preserved.push(line.to_string()),
                _ => {}
            }
        }

        let category = parse_category(fields.get("category").map(String::as_str));
        if category.is_none() {
            warnings.push(warning(
                "missing_or_invalid_category",
                "Missing or invalid Category field.",
                None,
            ));
        }
        let area = fields
            .get("area")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if area.is_none() {
            warnings.push(warning("missing_area", "Missing Area field.", None));
        }
        let summary = summary.join("\n").trim().to_string();
        let limitations = limitations.join("\n").trim().to_string();
        if summary.is_empty() {
            warnings.push(warning("missing_summary", "Missing Summary section.", None));
        }
        if limitations.is_empty() {
            warnings.push(warning(
                "missing_limitations",
                "Missing Limitations section.",
                None,
            ));
        }
        if let (Some(category), Some(area)) = (category, area)
            && !summary.is_empty()
            && !limitations.is_empty()
        {
            entries.push(WorkHistoryEntry {
                occurred_at,
                title,
                category,
                area,
                summary,
                limitations,
            });
        } else {
            preserved.push(block.raw.trim().to_string());
        }
    }
    let categories = entries
        .iter()
        .map(|entry| entry.category)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let areas = entries
        .iter()
        .map(|entry| entry.area.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if !preserved.is_empty() {
        warnings.push(warning(
            "preserved_unrecognized_content",
            "Unrecognized source content is preserved for review.",
            None,
        ));
    }
    WorkHistoryDocument {
        relative_path: None,
        entries,
        categories,
        areas,
        warnings: deduplicate_warnings(warnings),
        preserved_content: nonempty_join(preserved),
    }
}

pub fn render_work_history_document(document: &WorkHistoryDocument) -> String {
    let mut output = String::new();
    for entry in &document.entries {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&format!(
            "## {} — {}\n\n",
            entry.occurred_at.format("%Y-%m-%d %H:%M"),
            entry.title.trim()
        ));
        output.push_str(&format!("- Category: {}\n", work_category(entry.category)));
        output.push_str(&format!("- Area: {}\n\n", entry.area.trim()));
        output.push_str("### Summary\n\n");
        output.push_str(entry.summary.trim());
        output.push_str("\n\n### Limitations\n\n");
        output.push_str(entry.limitations.trim());
        output.push('\n');
    }
    if let Some(content) = document
        .preserved_content
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str("## Preserved Unrecognized Content\n\n");
        output.push_str(
            "The following source content was not recognized as structured working history and is preserved verbatim.\n\n",
        );
        output.push_str(content.trim());
        output.push('\n');
    }
    output
}

fn canonical_root(root: &Path) -> Result<PathBuf, ProjectStateError> {
    let root = root
        .canonicalize()
        .map_err(|error| ProjectStateError::InvalidRoot(error.to_string()))?;
    if !root.is_dir() {
        return Err(ProjectStateError::InvalidRoot(
            root.to_string_lossy().into_owned(),
        ));
    }
    Ok(root)
}

fn find_document(root: &Path, aliases: &[&str]) -> Result<Option<PathBuf>, ProjectStateError> {
    for directory in [root.to_path_buf(), root.join("docs")] {
        if !directory.exists() {
            continue;
        }
        let canonical = directory.canonicalize()?;
        if !canonical.starts_with(root) {
            return Err(ProjectStateError::InvalidRoot(
                "A document directory resolves outside the registered root".into(),
            ));
        }
        for entry in fs::read_dir(&canonical)? {
            let path = entry?.path();
            if aliases.iter().any(|alias| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.eq_ignore_ascii_case(alias))
            }) {
                let resolved = path.canonicalize()?;
                if !resolved.starts_with(root) || !resolved.is_file() {
                    return Err(ProjectStateError::InvalidRoot(
                        "A project-state document resolves outside the registered root".into(),
                    ));
                }
                return Ok(Some(resolved));
            }
        }
    }
    Ok(None)
}

fn writable_document_path(
    root: &Path,
    default_name: &str,
    aliases: &[&str],
) -> Result<PathBuf, ProjectStateError> {
    Ok(find_document(root, aliases)?.unwrap_or_else(|| root.join(default_name)))
}

fn relative_path(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|value| value.to_string_lossy().replace('\\', "/"))
}

fn read_optional(path: &Path) -> Result<Option<String>, io::Error> {
    if path.exists() {
        fs::read_to_string(path).map(Some)
    } else {
        Ok(None)
    }
}

pub(crate) fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), io::Error> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Target file has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file_mut().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    #[cfg(not(windows))]
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn replace_pair_with_rollback(
    first: (&Path, &[u8], Option<&str>),
    second: (&Path, &[u8], Option<&str>),
) -> Result<(), ProjectStateError> {
    atomic_replace(first.0, first.1)?;
    if let Err(error) = atomic_replace(second.0, second.1) {
        match first.2 {
            Some(original) => atomic_replace(first.0, original.as_bytes())?,
            None if first.0.exists() => fs::remove_file(first.0)?,
            None => {}
        }
        return Err(ProjectStateError::Io(error));
    }
    Ok(())
}

fn restore_pair(
    first: (&Path, Option<&str>),
    second: (&Path, Option<&str>),
) -> Result<(), ProjectStateError> {
    restore_file(first.0, first.1)?;
    restore_file(second.0, second.1)?;
    Ok(())
}

fn restore_file(path: &Path, original: Option<&str>) -> Result<(), ProjectStateError> {
    match original {
        Some(original) => atomic_replace(path, original.as_bytes())?,
        None if path.exists() => fs::remove_file(path)?,
        None => {}
    }
    Ok(())
}

fn ensure_mutable_todo_document(document: &TodoDocument) -> Result<(), ProjectStateError> {
    let fatal: Vec<_> = document
        .warnings
        .iter()
        .filter(|warning| {
            matches!(
                warning.code.as_str(),
                "duplicate_todo_id"
                    | "invalid_todo_id"
                    | "missing_dependency"
                    | "circular_dependency"
                    | "missing_priority"
                    | "invalid_priority"
                    | "missing_or_invalid_category"
                    | "missing_area"
                    | "missing_rationale"
                    | "missing_acceptance_criteria"
            )
        })
        .map(|warning| warning.message.as_str())
        .collect();
    if fatal.is_empty() {
        Ok(())
    } else {
        Err(ProjectStateError::Validation(fatal.join(" ")))
    }
}

fn ensure_mutable_history_document(
    document: &WorkHistoryDocument,
) -> Result<(), ProjectStateError> {
    if document.warnings.iter().any(|warning| {
        matches!(
            warning.code.as_str(),
            "missing_or_invalid_category"
                | "missing_area"
                | "missing_summary"
                | "missing_limitations"
        )
    }) {
        Err(ProjectStateError::Validation(
            "Working history contains malformed entries; migrate or repair it before mutation"
                .into(),
        ))
    } else {
        Ok(())
    }
}

fn next_todo_id(items: &[TodoItem]) -> String {
    let next = items
        .iter()
        .filter_map(|item| item.id.strip_prefix("TODO-")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    format!("TODO-{next:03}")
}

fn validate_nonempty(name: &str, value: &str) -> Result<(), ProjectStateError> {
    if value.trim().is_empty() {
        Err(ProjectStateError::Validation(format!(
            "{name} must not be empty"
        )))
    } else {
        Ok(())
    }
}

fn local_minute() -> NaiveDateTime {
    Local::now()
        .naive_local()
        .with_second(0)
        .and_then(|value| value.with_nanosecond(0))
        .unwrap_or_else(|| Local::now().naive_local())
}

fn validate_todo_ids(values: &[String], name: &str) -> Result<(), ProjectStateError> {
    for value in values {
        validate_todo_id(value).map_err(|_| {
            ProjectStateError::Validation(format!("{name} contains invalid TODO ID `{value}`"))
        })?;
    }
    Ok(())
}

fn validate_todo_id(value: &str) -> Result<(), ProjectStateError> {
    if is_todo_id(value) {
        Ok(())
    } else {
        Err(ProjectStateError::Validation(format!(
            "`{value}` is not a valid TODO ID"
        )))
    }
}

fn is_todo_id(value: &str) -> bool {
    value.strip_prefix("TODO-").is_some_and(|digits| {
        digits.len() >= 3 && digits.chars().all(|character| character.is_ascii_digit())
    })
}

fn has_cycle<'a>(
    origin: &'a str,
    current: &'a str,
    graph: &HashMap<&'a str, &'a [String]>,
    visiting: &mut HashSet<&'a str>,
) -> bool {
    if !visiting.insert(current) {
        return current == origin;
    }
    let result = graph.get(current).is_some_and(|dependencies| {
        dependencies.iter().any(|dependency| {
            dependency == origin
                || graph.contains_key(dependency.as_str())
                    && has_cycle(origin, dependency, graph, visiting)
        })
    });
    visiting.remove(current);
    result
}

fn parse_todo_heading(heading: &str) -> Option<(String, String, bool)> {
    let heading = heading.trim().strip_prefix("## ")?;
    let rest = heading.strip_prefix("TODO-")?;
    let digit_count = rest
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .count();
    if digit_count < 3 {
        return None;
    }
    let id = format!("TODO-{}", &rest[..digit_count]);
    let suffix = rest[digit_count..].trim_start();
    if let Some(title) = suffix.strip_prefix(':') {
        return Some((id, title.trim().to_string(), true));
    }
    suffix
        .strip_prefix('-')
        .map(|title| (id, title.trim().to_string(), false))
}

fn parse_history_heading(heading: &str) -> Option<(NaiveDateTime, String, bool)> {
    let heading = heading.trim().strip_prefix("## ")?;
    if heading.len() < 10 {
        return None;
    }
    let (date_time, suffix, has_time) = if heading.len() >= 16
        && NaiveDateTime::parse_from_str(&heading[..16], "%Y-%m-%d %H:%M").is_ok()
    {
        (
            NaiveDateTime::parse_from_str(&heading[..16], "%Y-%m-%d %H:%M").ok()?,
            &heading[16..],
            true,
        )
    } else {
        let date = chrono::NaiveDate::parse_from_str(&heading[..10], "%Y-%m-%d").ok()?;
        (date.and_hms_opt(0, 0, 0)?, &heading[10..], false)
    };
    let suffix = suffix.trim_start();
    let (title, en_dash) = if let Some(title) = suffix.strip_prefix('—') {
        (title, true)
    } else if let Some(title) = suffix.strip_prefix('-') {
        (title, false)
    } else {
        return None;
    };
    Some((date_time, title.trim().to_string(), has_time && en_dash))
}

fn parse_field_line(line: &str) -> Option<(String, &str)> {
    let mut value = line.trim();
    value = value.strip_prefix('-').unwrap_or(value).trim_start();
    value = value.strip_prefix("**").unwrap_or(value);
    let (key, remainder) = value.split_once(':')?;
    let key = key.trim().trim_matches('*').to_lowercase();
    Some((
        key,
        remainder
            .trim()
            .trim_start_matches("**")
            .trim_end_matches("**")
            .trim(),
    ))
}

fn required_field(
    fields: &HashMap<String, String>,
    name: &str,
    id: &str,
    warnings: &mut Vec<ValidationWarning>,
) -> Option<String> {
    let value = fields
        .get(name)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if value.is_none() {
        warnings.push(warning(
            &format!("missing_{}", name.replace(' ', "_")),
            &format!("Missing {} field.", title_case(name)),
            Some(id),
        ));
    }
    value
}

fn parse_id_list(
    value: &str,
    item_id: &str,
    field_name: &str,
    warnings: &mut Vec<ValidationWarning>,
) -> Vec<String> {
    if value.trim().eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    let mut ids = Vec::new();
    for part in value.split(',') {
        let id = part.trim().to_uppercase();
        if is_todo_id(&id) {
            ids.push(id);
        } else {
            warnings.push(warning(
                "invalid_todo_reference",
                &format!("Invalid TODO ID `{}` in {field_name}.", part.trim()),
                (!item_id.is_empty()).then_some(item_id),
            ));
        }
    }
    deduplicate(ids)
}

fn parse_category(value: Option<&str>) -> Option<WorkCategory> {
    match value?.trim().to_lowercase().as_str() {
        "feature" => Some(WorkCategory::Feature),
        "bugfix" => Some(WorkCategory::Bugfix),
        "refactor" => Some(WorkCategory::Refactor),
        "test" => Some(WorkCategory::Test),
        "documentation" => Some(WorkCategory::Documentation),
        "research" => Some(WorkCategory::Research),
        "others" | "decision" => Some(WorkCategory::Others),
        _ => None,
    }
}

pub(crate) struct HeadingBlock<'a> {
    heading: &'a str,
    body: &'a str,
    raw: &'a str,
}

pub(crate) fn split_heading_blocks(content: &str) -> Vec<HeadingBlock<'_>> {
    let mut starts = Vec::new();
    let mut offset = 0;
    for line in content.split_inclusive('\n') {
        if line.starts_with("## ") {
            starts.push(offset);
        }
        offset += line.len();
    }
    if starts.first().copied().unwrap_or(usize::MAX) != 0
        && !content[..starts.first().copied().unwrap_or(content.len())]
            .trim()
            .is_empty()
    {
        starts.insert(0, 0);
    }
    if starts.is_empty() && !content.trim().is_empty() {
        starts.push(0);
    }
    let mut blocks = Vec::new();
    for (index, start) in starts.iter().copied().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(content.len());
        let raw = &content[start..end];
        let (heading, body) = raw.split_once('\n').unwrap_or((raw, ""));
        blocks.push(HeadingBlock { heading, body, raw });
    }
    blocks
}

fn preserved_block(block: &HeadingBlock<'_>) -> Option<String> {
    const INTRO: &str = "The following source content was not recognized as structured project state and is preserved verbatim.";
    const HISTORY_INTRO: &str = "The following source content was not recognized as structured working history and is preserved verbatim.";
    let content = if block
        .heading
        .trim()
        .eq_ignore_ascii_case("## Preserved Unrecognized Content")
    {
        block
            .body
            .trim()
            .strip_prefix(INTRO)
            .or_else(|| block.body.trim().strip_prefix(HISTORY_INTRO))
            .unwrap_or(block.body.trim())
            .trim()
    } else {
        block.raw.trim()
    };
    (!content.is_empty()).then(|| content.to_string())
}

fn warning(code: &str, message: &str, item_id: Option<&str>) -> ValidationWarning {
    ValidationWarning {
        code: code.to_string(),
        message: message.to_string(),
        item_id: item_id.map(str::to_string),
    }
}

fn deduplicate_warnings(warnings: Vec<ValidationWarning>) -> Vec<ValidationWarning> {
    let mut seen = HashSet::new();
    warnings
        .into_iter()
        .filter(|warning| {
            seen.insert((
                warning.code.clone(),
                warning.message.clone(),
                warning.item_id.clone(),
            ))
        })
        .collect()
}

fn deduplicate(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn normalize_newlines(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

fn nonempty_join(values: Vec<String>) -> Option<String> {
    let value = values
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (!value.trim().is_empty()).then_some(value)
}

fn title_case(value: &str) -> String {
    let mut characters = value.chars();
    characters
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + characters.as_str())
        .unwrap_or_default()
}

fn todo_priority(priority: TodoPriority) -> &'static str {
    match priority {
        TodoPriority::Critical => "critical",
        TodoPriority::High => "high",
        TodoPriority::Medium => "medium",
        TodoPriority::Low => "low",
    }
}

fn work_category(category: WorkCategory) -> &'static str {
    match category {
        WorkCategory::Feature => "feature",
        WorkCategory::Bugfix => "bugfix",
        WorkCategory::Refactor => "refactor",
        WorkCategory::Test => "test",
        WorkCategory::Documentation => "documentation",
        WorkCategory::Research => "research",
        WorkCategory::Others => "others",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    fn sample_todo(id: &str, priority: TodoPriority, dependencies: &[&str]) -> TodoItem {
        TodoItem {
            id: id.into(),
            title: format!("Task {id}"),
            priority,
            category: WorkCategory::Feature,
            area: "state".into(),
            dependencies: dependencies.iter().map(|value| value.to_string()).collect(),
            rationale: "Needed.".into(),
            acceptance_criteria: "The behavior is verified.".into(),
        }
    }

    #[test]
    fn todo_parser_writer_round_trip_is_deterministic() {
        let document = TodoDocument {
            relative_path: None,
            items: vec![sample_todo("TODO-001", TodoPriority::High, &[])],
            warnings: Vec::new(),
            preserved_content: None,
        };
        let rendered = render_todo_document(&document);
        let parsed = parse_todo_document(&rendered);
        assert_eq!(parsed.items, document.items);
        assert!(parsed.warnings.is_empty());
        assert_eq!(render_todo_document(&parsed), rendered);
    }

    #[test]
    fn legacy_bold_todo_fields_are_parsed_for_migration() {
        let parsed = parse_todo_document(
            "## TODO-002 - Legacy title\n\n\
             - **Priority:** Medium\n\
             - **Status:** Planned\n\
             - **Area:** Backend / adapters\n\
             - **Dependencies:** none\n\
             - **Rationale:** Needed.\n\
             - **Acceptance criteria:** Verified.",
        );
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].priority, TodoPriority::Medium);
        assert_eq!(parsed.items[0].category, WorkCategory::Others);
        assert_eq!(parsed.items[0].area, "Backend / adapters");
        assert!(!render_todo_document(&parsed).contains("- Status:"));
    }

    #[test]
    fn priority_order_and_dependency_failures_are_reported() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("TODO.md"),
            render_todo_document(&TodoDocument {
                relative_path: None,
                items: vec![
                    sample_todo("TODO-002", TodoPriority::Low, &["TODO-999"]),
                    sample_todo("TODO-001", TodoPriority::Critical, &["TODO-002"]),
                ],
                warnings: Vec::new(),
                preserved_content: None,
            }),
        )
        .unwrap();
        let state = inspect_project_state(temp.path()).unwrap();
        assert_eq!(state.todos.items[0].id, "TODO-001");
        assert!(
            state
                .todos
                .warnings
                .iter()
                .any(|warning| warning.code == "missing_dependency")
        );
    }

    #[test]
    fn cycles_are_reported_for_every_participant() {
        let warnings = validate_todos(&[
            sample_todo("TODO-001", TodoPriority::High, &["TODO-002"]),
            sample_todo("TODO-002", TodoPriority::High, &["TODO-001"]),
        ]);
        assert_eq!(
            warnings
                .iter()
                .filter(|warning| warning.code == "circular_dependency")
                .count(),
            2
        );
    }

    #[test]
    fn malformed_content_is_preserved_with_warning() {
        let parsed = parse_todo_document(
            "## TODO-001: Incomplete\n\n- Status: planned\n\nOriginal details.",
        );
        assert!(parsed.items.is_empty());
        assert!(
            parsed
                .preserved_content
                .unwrap()
                .contains("Original details")
        );
        assert!(!parsed.warnings.is_empty());
    }

    #[test]
    fn completion_proposal_is_persisted_without_changing_project_files() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let proposal_file = temp.path().join("app-data/completion-proposals.json");
        let service = ProjectStateService::load(proposal_file.clone()).unwrap();
        let project_id = Uuid::new_v4();
        let todo = render_todo_document(&TodoDocument {
            relative_path: None,
            items: vec![sample_todo("TODO-001", TodoPriority::High, &[])],
            warnings: Vec::new(),
            preserved_content: None,
        });
        fs::write(project.join("TODO.md"), &todo).unwrap();
        fs::write(project.join("WORK_HISTORY.md"), "").unwrap();

        let proposal = service
            .complete_todo(
                project_id,
                &project,
                "TODO-001",
                CompleteTodoInput {
                    summary: "Implemented it.".into(),
                    limitations: "none".into(),
                },
            )
            .unwrap();

        assert_eq!(proposal.kind, ProposalKind::TodoCompletion);
        assert_eq!(proposal.todo.as_ref().unwrap().id, "TODO-001");
        assert_eq!(proposal.proposed_entry.summary, "Implemented it.");
        assert_eq!(fs::read_to_string(project.join("TODO.md")).unwrap(), todo);
        assert_eq!(
            fs::read_to_string(project.join("WORK_HISTORY.md")).unwrap(),
            ""
        );
        let mut stored: serde_json::Value =
            serde_json::from_slice(&fs::read(&proposal_file).unwrap()).unwrap();
        stored["proposals"][0]
            .as_object_mut()
            .unwrap()
            .remove("kind");
        fs::write(&proposal_file, serde_json::to_vec_pretty(&stored).unwrap()).unwrap();
        let reloaded = ProjectStateService::load(proposal_file).unwrap();
        assert_eq!(
            reloaded.pending_reviews(project_id).unwrap(),
            vec![proposal]
        );
    }

    #[test]
    fn approving_proposal_updates_both_files_and_satisfies_dependents() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let proposal_file = temp.path().join("app-data/proposals.json");
        let service = ProjectStateService::load(proposal_file.clone()).unwrap();
        let project_id = Uuid::new_v4();
        fs::write(
            project.join("TODO.md"),
            render_todo_document(&TodoDocument {
                relative_path: None,
                items: vec![
                    sample_todo("TODO-001", TodoPriority::High, &[]),
                    sample_todo("TODO-002", TodoPriority::Medium, &["TODO-001"]),
                ],
                warnings: Vec::new(),
                preserved_content: None,
            }),
        )
        .unwrap();
        let proposal = service
            .complete_todo(
                project_id,
                &project,
                "TODO-001",
                CompleteTodoInput {
                    summary: "Implemented it.".into(),
                    limitations: "none".into(),
                },
            )
            .unwrap();
        let result = service
            .approve_completion(project_id, &project, proposal.id)
            .unwrap();
        assert_eq!(result.completed_todo.as_ref().unwrap().id, "TODO-001");
        let state = service.inspect(&project).unwrap();
        assert_eq!(state.todos.items.len(), 1);
        assert!(state.todos.items[0].dependencies.is_empty());
        assert_eq!(state.working_history.entries[0].title, "Task TODO-001");
        assert_eq!(
            state.working_history.entries[0].category,
            WorkCategory::Feature
        );
        assert_eq!(state.working_history.entries[0].area, "state");
        assert!(service.pending_reviews(project_id).unwrap().is_empty());
        assert!(
            ProjectStateService::load(proposal_file)
                .unwrap()
                .pending_reviews(project_id)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rejecting_proposal_leaves_todo_and_history_unchanged() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let proposal_file = temp.path().join("app-data/proposals.json");
        let service = ProjectStateService::load(proposal_file.clone()).unwrap();
        let project_id = Uuid::new_v4();
        let todo = render_todo_document(&TodoDocument {
            relative_path: None,
            items: vec![sample_todo("TODO-001", TodoPriority::High, &[])],
            warnings: Vec::new(),
            preserved_content: None,
        });
        fs::write(project.join("TODO.md"), &todo).unwrap();
        fs::write(project.join("WORK_HISTORY.md"), "").unwrap();
        let proposal = service
            .complete_todo(
                project_id,
                &project,
                "TODO-001",
                CompleteTodoInput {
                    summary: "No change.".into(),
                    limitations: "none".into(),
                },
            )
            .unwrap();

        service.reject_completion(project_id, proposal.id).unwrap();

        assert_eq!(fs::read_to_string(project.join("TODO.md")).unwrap(), todo);
        assert_eq!(
            fs::read_to_string(project.join("WORK_HISTORY.md")).unwrap(),
            ""
        );
        assert!(service.pending_reviews(project_id).unwrap().is_empty());
        assert!(
            ProjectStateService::load(proposal_file)
                .unwrap()
                .pending_reviews(project_id)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn working_history_request_uses_the_existing_proposal_store_and_approval_flow() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("WORK_HISTORY.md"), "").unwrap();
        let proposal_file = temp.path().join("app-data/proposals.json");
        let project_id = Uuid::new_v4();
        let service = ProjectStateService::load(proposal_file.clone()).unwrap();

        let proposal = service
            .add_work_history(
                project_id,
                &project,
                AddWorkHistoryInput {
                    title: "Standalone work".into(),
                    category: WorkCategory::Documentation,
                    area: "project-state".into(),
                    summary: "Prepared the entry.".into(),
                    limitations: "none".into(),
                },
            )
            .unwrap();

        assert_eq!(proposal.kind, ProposalKind::WorkHistory);
        assert!(proposal.todo.is_none());
        assert_eq!(
            fs::read_to_string(project.join("WORK_HISTORY.md")).unwrap(),
            ""
        );
        let reloaded = ProjectStateService::load(proposal_file).unwrap();
        assert_eq!(
            reloaded.pending_reviews(project_id).unwrap(),
            vec![proposal.clone()]
        );

        let result = reloaded
            .approve_completion(project_id, &project, proposal.id)
            .unwrap();
        assert!(result.completed_todo.is_none());
        assert_eq!(result.history_entry.title, "Standalone work");
        let state = reloaded.inspect(&project).unwrap();
        assert_eq!(state.working_history.entries.len(), 1);
        assert_eq!(
            state.working_history.entries[0].summary,
            "Prepared the entry."
        );
        assert!(reloaded.pending_reviews(project_id).unwrap().is_empty());
    }

    #[test]
    fn rejecting_working_history_proposal_discards_it_without_changing_history() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("WORK_HISTORY.md"), "Original history").unwrap();
        let service = ProjectStateService::default();
        let project_id = Uuid::new_v4();
        let proposal = service
            .add_work_history(
                project_id,
                &project,
                AddWorkHistoryInput {
                    title: "Discarded work".into(),
                    category: WorkCategory::Others,
                    area: "project-state".into(),
                    summary: "Do not append.".into(),
                    limitations: "none".into(),
                },
            )
            .unwrap();

        service.reject_completion(project_id, proposal.id).unwrap();

        assert_eq!(
            fs::read_to_string(project.join("WORK_HISTORY.md")).unwrap(),
            "Original history"
        );
        assert!(service.pending_reviews(project_id).unwrap().is_empty());
    }

    #[test]
    fn failed_working_history_approval_restores_history_and_retains_proposal() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let original = "Original history";
        fs::write(project.join("WORK_HISTORY.md"), original).unwrap();
        let proposal_file = temp.path().join("app-data/proposals.json");
        let service = ProjectStateService::load(proposal_file.clone()).unwrap();
        let project_id = Uuid::new_v4();
        let proposal = service
            .add_work_history(
                project_id,
                &project,
                AddWorkHistoryInput {
                    title: "Rollback work".into(),
                    category: WorkCategory::Test,
                    area: "project-state".into(),
                    summary: "Exercise rollback.".into(),
                    limitations: "none".into(),
                },
            )
            .unwrap();
        fs::remove_file(&proposal_file).unwrap();
        fs::create_dir(&proposal_file).unwrap();

        let result = service.approve_completion(project_id, &project, proposal.id);

        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(project.join("WORK_HISTORY.md")).unwrap(),
            original
        );
        assert_eq!(service.pending_reviews(project_id).unwrap(), vec![proposal]);
    }

    #[test]
    fn repeated_submission_is_rejected_until_the_pending_proposal_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let service = ProjectStateService::default();
        let project_id = Uuid::new_v4();
        fs::write(
            project.join("TODO.md"),
            render_todo_document(&TodoDocument {
                relative_path: None,
                items: vec![sample_todo("TODO-001", TodoPriority::High, &[])],
                warnings: Vec::new(),
                preserved_content: None,
            }),
        )
        .unwrap();
        let input = || CompleteTodoInput {
            summary: "Done.".into(),
            limitations: "none".into(),
        };

        let first = service
            .complete_todo(project_id, &project, "TODO-001", input())
            .unwrap();
        let duplicate = service.complete_todo(project_id, &project, "TODO-001", input());
        assert!(
            duplicate
                .unwrap_err()
                .to_string()
                .contains("already has a pending completion proposal")
        );
        assert_eq!(service.pending_reviews(project_id).unwrap().len(), 1);

        service.reject_completion(project_id, first.id).unwrap();
        let second = service
            .complete_todo(project_id, &project, "TODO-001", input())
            .unwrap();
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn paired_replacement_rolls_back_the_first_file_when_the_second_fails() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("TODO.md");
        let invalid_parent = temp.path().join("not-a-directory");
        let second = invalid_parent.join("WORK_HISTORY.md");
        fs::write(&first, "original TODO").unwrap();
        fs::write(&invalid_parent, "blocks directory creation").unwrap();

        let result = replace_pair_with_rollback(
            (&first, b"updated TODO", Some("original TODO")),
            (&second, b"new history", None),
        );

        assert!(result.is_err());
        assert_eq!(fs::read_to_string(first).unwrap(), "original TODO");
        assert!(!second.exists());
    }

    #[test]
    fn concurrent_adds_generate_unique_ids_without_corruption() {
        let temp = tempfile::tempdir().unwrap();
        let service = Arc::new(ProjectStateService::default());
        let root = Arc::new(temp.path().to_path_buf());
        let handles: Vec<_> = (0..8)
            .map(|index| {
                let service = Arc::clone(&service);
                let root = Arc::clone(&root);
                thread::spawn(move || {
                    service
                        .add_todo(
                            &root,
                            AddTodoInput {
                                title: format!("Task {index}"),
                                priority: TodoPriority::Medium,
                                category: WorkCategory::Others,
                                area: "state".into(),
                                dependencies: Vec::new(),
                                rationale: "Needed.".into(),
                                acceptance_criteria: "Done.".into(),
                            },
                        )
                        .unwrap()
                        .id
                })
            })
            .collect();
        let ids: HashSet<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        assert_eq!(ids.len(), 8);
        assert_eq!(service.inspect(&root).unwrap().todos.items.len(), 8);
    }
}
