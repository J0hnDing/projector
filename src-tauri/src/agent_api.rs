use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener},
    sync::{Arc, Mutex},
};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    models::{
        AddTodoInput, AddWorkHistoryInput, CompleteTodoInput, CompleteTodoResult, ProjectState,
        RegistryEntry, TodoItem, WorkHistoryEntry,
    },
    project_state::{ProjectStateError, ProjectStateService},
    registry::RegistryStore,
};

pub const AGENT_API_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 48721);

#[derive(Clone)]
pub struct AgentApiContext {
    pub registry: Arc<Mutex<RegistryStore>>,
    pub project_state: Arc<ProjectStateService>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddTodoRequest {
    project_id: Uuid,
    #[serde(flatten)]
    input: AddTodoInput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompleteTodoRequest {
    project_id: Uuid,
    #[serde(flatten)]
    input: CompleteTodoInput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddWorkHistoryRequest {
    project_id: Uuid,
    #[serde(flatten)]
    input: AddWorkHistoryInput,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiProject {
    id: Uuid,
    name: String,
}

#[derive(Debug, Serialize)]
struct Health {
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    error: ApiErrorDetail,
}

#[derive(Debug, Serialize)]
struct ApiErrorDetail {
    code: &'static str,
    message: String,
}

struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ApiErrorBody {
                error: ApiErrorDetail {
                    code: self.code,
                    message: self.message,
                },
            }),
        )
            .into_response()
    }
}

pub fn start(context: AgentApiContext) -> Result<(), String> {
    let listener = TcpListener::bind(AGENT_API_ADDR).map_err(|error| {
        format!("Unable to bind the local agent API at http://{AGENT_API_ADDR}: {error}")
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("Unable to configure the local agent API: {error}"))?;
    let router = router(context);
    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::from_std(listener) {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("Unable to start the Projector local agent API: {error}");
                return;
            }
        };
        if let Err(error) = axum::serve(listener, router).await {
            eprintln!("Projector local agent API stopped: {error}");
        }
    });
    Ok(())
}

fn router(context: AgentApiContext) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/projects", get(list_projects))
        .route("/v1/projects/{project_id}/state", get(project_state))
        .route("/v1/add_todo", post(add_todo))
        .route("/v1/complete_todo", post(complete_todo))
        .route("/v1/add_work_history", post(add_work_history))
        .with_state(context)
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

async fn list_projects(
    State(context): State<AgentApiContext>,
) -> Result<Json<Vec<ApiProject>>, ApiError> {
    let registry = context.registry.lock().map_err(registry_unavailable)?;
    Ok(Json(
        registry
            .entries()
            .iter()
            .map(|entry| ApiProject {
                id: entry.id,
                name: entry.name.clone(),
            })
            .collect(),
    ))
}

async fn project_state(
    Path(project_id): Path<Uuid>,
    State(context): State<AgentApiContext>,
) -> Result<Json<ProjectState>, ApiError> {
    let entry = registered_project(&context, project_id)?;
    context
        .project_state
        .inspect(&entry.path)
        .map(Json)
        .map_err(state_error)
}

async fn add_todo(
    State(context): State<AgentApiContext>,
    Json(request): Json<AddTodoRequest>,
) -> Result<(StatusCode, Json<TodoItem>), ApiError> {
    let entry = registered_project(&context, request.project_id)?;
    context
        .project_state
        .add_todo(&entry.path, request.input)
        .map(|item| (StatusCode::CREATED, Json(item)))
        .map_err(state_error)
}

async fn complete_todo(
    State(context): State<AgentApiContext>,
    Json(request): Json<CompleteTodoRequest>,
) -> Result<Json<CompleteTodoResult>, ApiError> {
    let entry = registered_project(&context, request.project_id)?;
    context
        .project_state
        .complete_todo(&entry.path, request.input)
        .map(Json)
        .map_err(state_error)
}

async fn add_work_history(
    State(context): State<AgentApiContext>,
    Json(request): Json<AddWorkHistoryRequest>,
) -> Result<(StatusCode, Json<WorkHistoryEntry>), ApiError> {
    let entry = registered_project(&context, request.project_id)?;
    context
        .project_state
        .add_work_history(&entry.path, request.input)
        .map(|entry| (StatusCode::CREATED, Json(entry)))
        .map_err(state_error)
}

fn registered_project(context: &AgentApiContext, id: Uuid) -> Result<RegistryEntry, ApiError> {
    let registry = context.registry.lock().map_err(registry_unavailable)?;
    registry.find(id).cloned().ok_or_else(|| ApiError {
        status: StatusCode::NOT_FOUND,
        code: "unknown_project",
        message: format!("Unknown registered project id {id}"),
    })
}

fn registry_unavailable<T>(_: std::sync::PoisonError<T>) -> ApiError {
    ApiError {
        status: StatusCode::SERVICE_UNAVAILABLE,
        code: "registry_unavailable",
        message: "The project registry is unavailable".into(),
    }
}

fn state_error(error: ProjectStateError) -> ApiError {
    match error {
        ProjectStateError::Validation(message) => ApiError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "validation_failed",
            message,
        },
        ProjectStateError::InvalidRoot(message) => ApiError {
            status: StatusCode::FORBIDDEN,
            code: "invalid_project_root",
            message,
        },
        ProjectStateError::Io(error) => ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "write_failed",
            message: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use serde_json::{Value, json};
    use tempfile::tempdir;
    use tower::ServiceExt;

    fn test_context() -> (tempfile::TempDir, AgentApiContext, Uuid) {
        let temp = tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let mut registry =
            RegistryStore::load(temp.path().join("registered-projects.json")).unwrap();
        let entry = registry.register(&project).unwrap();
        (
            temp,
            AgentApiContext {
                registry: Arc::new(Mutex::new(registry)),
                project_state: Arc::new(ProjectStateService::default()),
            },
            entry.id,
        )
    }

    async fn json_request(app: Router, route: &str, value: Value) -> (StatusCode, Value) {
        let response = app
            .oneshot(
                Request::post(route)
                    .header("content-type", "application/json")
                    .body(Body::from(value.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn api_supports_all_three_mutations_through_the_shared_service() {
        let (_temp, context, project_id) = test_context();
        let app = router(context.clone());
        let (status, todo) = json_request(
            app.clone(),
            "/v1/add_todo",
            json!({
                "projectId": project_id,
                "title": "Structured state",
                "priority": "high",
                "area": "project-state",
                "dependencies": [],
                "rationale": "Agents need a bounded contract.",
                "acceptanceCriteria": "The operation is tested."
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(todo["id"], "TODO-001");

        let (status, _) = json_request(
            app.clone(),
            "/v1/add_work_history",
            json!({
                "projectId": project_id,
                "title": "Research recorded",
                "category": "research",
                "relatedTodos": [],
                "area": "project-state",
                "summary": "Investigated the contract.",
                "limitations": "none"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);

        let (status, completed) = json_request(
            app,
            "/v1/complete_todo",
            json!({
                "projectId": project_id,
                "todoId": "TODO-001",
                "historyTitle": "Structured state implemented",
                "category": "feature",
                "area": "project-state",
                "summary": "Implemented the contract.",
                "limitations": "none"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(completed["completedTodo"]["id"], "TODO-001");
        assert_eq!(completed["historyEntry"]["relatedTodos"][0], "TODO-001");
    }

    #[tokio::test]
    async fn unregistered_projects_are_rejected() {
        let (_temp, context, _) = test_context();
        let (status, body) = json_request(
            router(context),
            "/v1/add_todo",
            json!({
                "projectId": Uuid::new_v4(),
                "title": "No",
                "priority": "low",
                "area": "state",
                "dependencies": [],
                "rationale": "No",
                "acceptanceCriteria": "No"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "unknown_project");
    }
}
