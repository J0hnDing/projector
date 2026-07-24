mod agent_api;
mod git_sync;
pub mod migration;
mod models;
mod observer;
pub mod project_state;
mod registry;
mod watcher;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use git_sync::GitSyncManager;
use models::{
    AddTodoInput, AddWorkHistoryInput, CompleteTodoInput, CompleteTodoResult, ProjectDetail,
    ProjectSummary, TodoItem, WorkHistoryEntry,
};
use project_state::ProjectStateService;
use registry::RegistryStore;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;
use watcher::ProjectWatcher;

struct AppState {
    registry: Arc<Mutex<RegistryStore>>,
    watcher: Mutex<ProjectWatcher>,
    git_sync: GitSyncManager,
    project_state: Arc<ProjectStateService>,
}

#[tauri::command]
async fn list_projects(state: State<'_, AppState>) -> Result<Vec<ProjectSummary>, String> {
    let entries = state
        .registry
        .lock()
        .map_err(|_| "The project registry is unavailable".to_string())?
        .entries()
        .to_vec();
    inspect_projects(entries, state.git_sync.clone()).await
}

async fn inspect_projects(
    entries: Vec<models::RegistryEntry>,
    git_sync: GitSyncManager,
) -> Result<Vec<ProjectSummary>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut projects: Vec<_> = entries
            .iter()
            .map(|entry| observer::summarize(entry, &git_sync.snapshot(entry.id)))
            .collect();
        projects.sort_by(|left, right| {
            right
                .last_opened
                .cmp(&left.last_opened)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        projects
    })
    .await
    .map_err(|error| format!("Unable to inspect projects: {error}"))
}

#[tauri::command]
async fn register_project(
    path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ProjectSummary, String> {
    let entry = state
        .registry
        .lock()
        .map_err(|_| "The project registry is unavailable".to_string())?
        .register(&PathBuf::from(path))
        .map_err(|error| error.to_string())?;

    state
        .watcher
        .lock()
        .map_err(|_| "The project watcher is unavailable".to_string())?
        .watch(&entry.path)
        .map_err(|error| {
            format!("Project was registered, but automatic refresh could not start: {error}")
        })?;

    let fetch_entry = entry.clone();
    let snapshot = state.git_sync.snapshot(entry.id);
    let project =
        tauri::async_runtime::spawn_blocking(move || observer::summarize(&entry, &snapshot))
            .await
            .map_err(|error| format!("Unable to inspect project: {error}"))?;
    state.git_sync.fetch(app, fetch_entry);
    Ok(project)
}

#[tauri::command]
fn remove_project(id: Uuid, state: State<'_, AppState>) -> Result<(), String> {
    let removed = state
        .registry
        .lock()
        .map_err(|_| "The project registry is unavailable".to_string())?
        .remove(id)
        .map_err(|error| error.to_string())?;

    if let Some(entry) = removed {
        state
            .watcher
            .lock()
            .map_err(|_| "The project watcher is unavailable".to_string())?
            .unwatch(&entry.path);
        state.git_sync.remove(id);
        Ok(())
    } else {
        Err(format!("Unknown project id {id}"))
    }
}

#[tauri::command]
async fn open_project(id: Uuid, state: State<'_, AppState>) -> Result<ProjectDetail, String> {
    let entry = state
        .registry
        .lock()
        .map_err(|_| "The project registry is unavailable".to_string())?
        .touch(id)
        .map_err(|error| error.to_string())?;

    let snapshot = state.git_sync.snapshot(entry.id);
    tauri::async_runtime::spawn_blocking(move || observer::detail(&entry, &snapshot))
        .await
        .map_err(|error| format!("Unable to inspect project: {error}"))
}

#[tauri::command]
async fn refresh_project(id: Uuid, state: State<'_, AppState>) -> Result<ProjectDetail, String> {
    let entry = state
        .registry
        .lock()
        .map_err(|_| "The project registry is unavailable".to_string())?
        .find(id)
        .cloned()
        .ok_or_else(|| format!("Unknown project id {id}"))?;

    let snapshot = state.git_sync.snapshot(entry.id);
    tauri::async_runtime::spawn_blocking(move || observer::detail(&entry, &snapshot))
        .await
        .map_err(|error| format!("Unable to inspect project: {error}"))
}

#[tauri::command]
async fn refresh_projects(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ProjectSummary>, String> {
    let entries = state
        .registry
        .lock()
        .map_err(|_| "The project registry is unavailable".to_string())?
        .entries()
        .to_vec();
    for entry in &entries {
        state.git_sync.fetch(app.clone(), entry.clone());
    }
    inspect_projects(entries, state.git_sync.clone()).await
}

#[tauri::command]
async fn pull_project(id: Uuid, state: State<'_, AppState>) -> Result<ProjectDetail, String> {
    let entry = state
        .registry
        .lock()
        .map_err(|_| "The project registry is unavailable".to_string())?
        .find(id)
        .cloned()
        .ok_or_else(|| format!("Unknown project id {id}"))?;
    let git_sync = state.git_sync.clone();

    tauri::async_runtime::spawn_blocking(move || {
        git_sync.pull(&entry)?;
        Ok(observer::detail(&entry, &git_sync.snapshot(entry.id)))
    })
    .await
    .map_err(|error| format!("Unable to pull project: {error}"))?
}

#[tauri::command]
async fn add_todo(
    id: Uuid,
    input: AddTodoInput,
    state: State<'_, AppState>,
) -> Result<TodoItem, String> {
    let root = registered_root(id, &state)?;
    let service = Arc::clone(&state.project_state);
    tauri::async_runtime::spawn_blocking(move || service.add_todo(&root, input))
        .await
        .map_err(|error| format!("Unable to add TODO: {error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn complete_todo(
    id: Uuid,
    input: CompleteTodoInput,
    state: State<'_, AppState>,
) -> Result<CompleteTodoResult, String> {
    let root = registered_root(id, &state)?;
    let service = Arc::clone(&state.project_state);
    tauri::async_runtime::spawn_blocking(move || service.complete_todo(&root, input))
        .await
        .map_err(|error| format!("Unable to complete TODO: {error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn add_work_history(
    id: Uuid,
    input: AddWorkHistoryInput,
    state: State<'_, AppState>,
) -> Result<WorkHistoryEntry, String> {
    let root = registered_root(id, &state)?;
    let service = Arc::clone(&state.project_state);
    tauri::async_runtime::spawn_blocking(move || service.add_work_history(&root, input))
        .await
        .map_err(|error| format!("Unable to add working history: {error}"))?
        .map_err(|error| error.to_string())
}

fn registered_root(id: Uuid, state: &State<'_, AppState>) -> Result<PathBuf, String> {
    state
        .registry
        .lock()
        .map_err(|_| "The project registry is unavailable".to_string())?
        .find(id)
        .map(|entry| entry.path.clone())
        .ok_or_else(|| format!("Unknown project id {id}"))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let registry_path = app.path().app_data_dir()?.join("registered-projects.json");
            let sync_cache_path = app.path().app_data_dir()?.join("git-sync-cache.json");
            let registry = RegistryStore::load(registry_path)
                .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            let mut watcher = ProjectWatcher::new(app.handle().clone())?;
            for entry in registry.entries() {
                if entry.path.is_dir() {
                    let _ = watcher.watch(&entry.path);
                }
            }
            let startup_entries = registry.entries().to_vec();
            let git_sync = GitSyncManager::load(sync_cache_path);
            let registry = Arc::new(Mutex::new(registry));
            let project_state = Arc::new(ProjectStateService::default());
            agent_api::start(agent_api::AgentApiContext {
                registry: Arc::clone(&registry),
                project_state: Arc::clone(&project_state),
            })
            .map_err(Box::<dyn std::error::Error>::from)?;
            app.manage(AppState {
                registry,
                watcher: Mutex::new(watcher),
                git_sync: git_sync.clone(),
                project_state,
            });
            for entry in startup_entries {
                git_sync.fetch(app.handle().clone(), entry);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_projects,
            register_project,
            remove_project,
            open_project,
            refresh_project,
            refresh_projects,
            pull_project,
            add_todo,
            complete_todo,
            add_work_history
        ])
        .run(tauri::generate_context!())
        .expect("error while running Projector");
}
