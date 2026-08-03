pub const PROJECTOR_SECTION: &str = r#"## Projector

Projector is authoritative for structured TODO and working-history mutations. Do not directly edit `TODO.md` or `WORK_HISTORY.md`.

Use Projector's local API at `http://127.0.0.1:48721/v1`. Projector must be running; resolve the registered project ID with `GET /projects`.

- `POST /projects/{projectId}/todos`: `title`, `priority` (`critical|high|medium|low`), `category` (`feature|bugfix|refactor|test|documentation|research|others`), `area`, `dependencies` (TODO ID array), `rationale`, and `acceptanceCriteria`.
- `POST /projects/{projectId}/todos/{todoId}/complete`: `summary` and `limitations`; returns a pending proposal with `id`, `projectId`, `requestedAt`, `kind: "todoCompletion"`, the `todo` snapshot, and `proposedEntry`.
- `POST /projects/{projectId}/work-history`: `title`, `category`, `area`, `summary`, and `limitations`; returns a pending proposal with `id`, `projectId`, `requestedAt`, `kind: "workHistory"`, `todo: null`, and `proposedEntry`.

Use `POST /projects/{projectId}/todos` to record unfinished actionable work. Use `POST /projects/{projectId}/todos/{todoId}/complete` when that TODO is finished. Use `POST /projects/{projectId}/work-history` only for notable completed work that was not represented by an open TODO.

Send JSON with camel-case field names. Use an empty array when a TODO has no dependencies and use `none` when there are no known limitations.
"#;

pub fn new_project_agents(project_name: &str, subagents_section: &str) -> String {
    format!(
        "# AGENTS.md instructions for {project_name}\n\n\
Project files are the source of truth. Keep changes scoped to this project and preserve unrelated work.\n\n\
{}\n\n\
{PROJECTOR_SECTION}",
        subagents_section.trim()
    )
}
