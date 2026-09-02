pub const PROJECTOR_SECTION: &str = r#"## Projector

Projector is authoritative for structured TODO and working-history mutations. Do not directly edit `TODO.md` or `WORK_HISTORY.md`.

Use Projector's local API at `http://127.0.0.1:48721/v1`. Projector must be running; resolve the registered project ID with `GET /projects`.

- `POST /projects/{projectId}/todos`: `title`, `priority` (`critical|high|medium|low`), `category` (`feature|bugfix|refactor|test|documentation|research|others`), `area`, `dependencies` (TODO ID array), `rationale`, and `acceptanceCriteria`.
- `POST /projects/{projectId}/todos/{todoId}/complete`: `summary` and `limitations`; returns a pending proposal with `id`, `projectId`, `requestedAt`, `kind: "todoCompletion"`, the `todo` snapshot, and `proposedEntry`.
- `POST /projects/{projectId}/work-history`: `title`, `category`, `area`, `summary`, and `limitations`; returns a pending proposal with `id`, `projectId`, `requestedAt`, `kind: "workHistory"`, `todo: null`, and `proposedEntry`.

Use `POST /projects/{projectId}/todos` to record unfinished actionable work. Use `POST /projects/{projectId}/todos/{todoId}/complete` when that TODO is finished. Use `POST /projects/{projectId}/work-history` only for notable completed work that was not represented by an open TODO.

Send JSON with camel-case field names. Use an empty array when a TODO has no dependencies and use `none` when there are no known limitations.
"#;

pub fn new_project_agents(
    project_name: &str,
    custom_section: &str,
    subagents_section: &str,
    projector_section: &str,
) -> String {
    format!(
        "# AGENTS.md instructions for {project_name}\n\n\
Project files are the source of truth. Keep changes scoped to this project and preserve unrelated work.\n\n\
{}\n\n{}\n\n{}\n",
        custom_section.trim(),
        subagents_section.trim(),
        projector_section.trim()
    )
}

pub fn migrate_agents(
    existing: Option<&str>,
    project_name: &str,
    custom_section: &str,
    subagents_section: &str,
    projector_section: &str,
) -> String {
    let Some(existing) = existing else {
        return new_project_agents(
            project_name,
            custom_section,
            subagents_section,
            projector_section,
        );
    };
    let updated = replace_section(existing, "Custom instructions", custom_section);
    let updated = replace_section(&updated, "Subagents", subagents_section);
    replace_section(&updated, "Projector", projector_section)
}

fn replace_section(content: &str, title: &str, section: &str) -> String {
    let heading = format!("## {title}");
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let mut output = Vec::new();
    let mut lines = normalized.lines().peekable();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_replaces_managed_sections_and_preserves_other_guidance() {
        let existing = "# Instructions\n\n## Existing\n\nKeep.\n\n## Subagents\n\nOld workers.\n\n## Projector\n\nOld API.\n\n## Later\n\nAlso keep.\n";
        let updated = migrate_agents(
            Some(existing),
            "Example",
            "## Custom instructions\n\nUse concise prose.",
            "## Subagents\n\nNew workers.",
            "## Projector\n\nNew API.",
        );
        assert!(updated.contains("## Existing\n\nKeep."));
        assert!(updated.contains("## Custom instructions\n\nUse concise prose."));
        assert!(updated.contains("## Subagents\n\nNew workers."));
        assert!(updated.contains("## Projector\n\nNew API."));
        assert!(updated.contains("## Later\n\nAlso keep."));
        assert!(!updated.contains("Old workers"));
        assert!(!updated.contains("Old API"));
    }
}
