pub const PROJECTOR_SECTION: &str = r#"## Projector

Projector is authoritative for structured TODO and working-history mutations. Do not directly edit `TODO.md` or `WORK_HISTORY.md`.

Use Projector's local API at `http://127.0.0.1:48721/v1`. Projector must be running; resolve the registered project ID with `GET /projects`.

- `POST /projects/{projectId}/todos`: `title`, `priority` (`critical|high|medium|low`), `category` (`feature|bugfix|refactor|test|documentation|research|others`), `area`, `dependencies` (TODO ID array), `rationale`, and `acceptanceCriteria`.
- `POST /projects/{projectId}/todos/{todoId}/complete`: `summary` and `limitations`.
- `POST /projects/{projectId}/work-history`: `title`, `category`, `area`, `summary`, and `limitations`.

Use `POST /projects/{projectId}/todos` to record unfinished actionable work. Use `POST /projects/{projectId}/todos/{todoId}/complete` when that TODO is finished; completion atomically removes the TODO and creates its working-history entry, so do not follow it with a `work-history` call. Use `POST /projects/{projectId}/work-history` only for notable completed work that was not represented by an open TODO.

Send JSON with camel-case field names. Use an empty array when a TODO has no dependencies and use `none` when there are no known limitations. TODO status is derived: a TODO with dependencies is blocked; otherwise it is planned.
"#;

pub fn new_project_agents(project_name: &str) -> String {
    format!(
        "# AGENTS.md instructions for {project_name}\n\n\
Project files are the source of truth. Keep changes scoped to this project and preserve unrelated work.\n\n\
{PROJECTOR_SECTION}"
    )
}
