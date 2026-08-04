mod agent_api;
mod agent_instructions;
mod codex_monitor;
mod git_sync;
pub mod migration;
mod models;
mod observer;
pub mod project_state;
mod registry;
mod subagent_settings;
mod watcher;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use codex_monitor::{CodexMonitorStore, CodexMonitoringSnapshot};
use git_sync::GitSyncManager;
use models::{
    AddTodoInput, AddWorkHistoryInput, CompleteTodoInput, CompleteTodoResult, CompletionProposal,
    ProjectDetail, ProjectSummary, TodoItem,
};
use project_state::ProjectStateService;
use registry::RegistryStore;
use subagent_settings::{GeneratedSubagentFile, SubagentSettings, SubagentSettingsStore};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;
use watcher::ProjectWatcher;

struct AppState {
    registry: Arc<Mutex<RegistryStore>>,
    watcher: Mutex<ProjectWatcher>,
    git_sync: GitSyncManager,
    project_state: Arc<ProjectStateService>,
    subagent_settings: Mutex<SubagentSettingsStore>,
    codex_monitor: Arc<Mutex<CodexMonitorStore>>,
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
async fn create_project(
    parent_path: String,
    name: String,
    initialize_git: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ProjectSummary, String> {
    let subagent_settings = state
        .subagent_settings
        .lock()
        .map_err(|_| "The subagent settings are unavailable".to_string())?
        .settings()
        .clone();
    let entry = state
        .registry
        .lock()
        .map_err(|_| "The project registry is unavailable".to_string())?
        .create_and_register(
            &PathBuf::from(parent_path),
            &name,
            initialize_git,
            &subagent_settings,
        )
        .map_err(|error| error.to_string())?;

    state
        .watcher
        .lock()
        .map_err(|_| "The project watcher is unavailable".to_string())?
        .watch(&entry.path)
        .map_err(|error| {
            format!("Project was created, but automatic refresh could not start: {error}")
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
fn get_subagent_settings(state: State<'_, AppState>) -> Result<SubagentSettings, String> {
    Ok(state
        .subagent_settings
        .lock()
        .map_err(|_| "The subagent settings are unavailable".to_string())?
        .settings()
        .clone())
}

#[tauri::command]
fn save_subagent_settings(
    settings: SubagentSettings,
    state: State<'_, AppState>,
) -> Result<SubagentSettings, String> {
    state
        .subagent_settings
        .lock()
        .map_err(|_| "The subagent settings are unavailable".to_string())?
        .save(settings)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn reset_subagent_settings(state: State<'_, AppState>) -> Result<SubagentSettings, String> {
    state
        .subagent_settings
        .lock()
        .map_err(|_| "The subagent settings are unavailable".to_string())?
        .reset()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn preview_subagent_files(
    settings: SubagentSettings,
    state: State<'_, AppState>,
) -> Result<Vec<GeneratedSubagentFile>, String> {
    state
        .subagent_settings
        .lock()
        .map_err(|_| "The subagent settings are unavailable".to_string())?
        .preview(&settings)
        .map_err(|error| error.to_string())
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
        state
            .codex_monitor
            .lock()
            .map_err(|_| "Codex monitoring is unavailable".to_string())?
            .remove_project(id)
            .map_err(|error| {
                format!("Project was removed, but Codex monitoring cleanup failed: {error}")
            })?;
        Ok(())
    } else {
        Err(format!("Unknown project id {id}"))
    }
}

#[tauri::command]
fn list_codex_sessions(
    id: Uuid,
    state: State<'_, AppState>,
) -> Result<CodexMonitoringSnapshot, String> {
    registered_root(id, &state)?;
    Ok(state
        .codex_monitor
        .lock()
        .map_err(|_| "Codex monitoring is unavailable".to_string())?
        .snapshot(id))
}

#[tauri::command]
fn link_codex_session(
    id: Uuid,
    session_id: String,
    state: State<'_, AppState>,
) -> Result<CodexMonitoringSnapshot, String> {
    registered_root(id, &state)?;
    let mut monitor = state
        .codex_monitor
        .lock()
        .map_err(|_| "Codex monitoring is unavailable".to_string())?;
    monitor
        .link(id, &session_id)
        .map_err(|error| error.to_string())?;
    Ok(monitor.snapshot(id))
}

#[tauri::command]
fn unlink_codex_session(
    id: Uuid,
    session_id: String,
    state: State<'_, AppState>,
) -> Result<CodexMonitoringSnapshot, String> {
    registered_root(id, &state)?;
    let mut monitor = state
        .codex_monitor
        .lock()
        .map_err(|_| "Codex monitoring is unavailable".to_string())?;
    monitor
        .unlink(id, &session_id)
        .map_err(|error| error.to_string())?;
    Ok(monitor.snapshot(id))
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
    let service = Arc::clone(&state.project_state);
    tauri::async_runtime::spawn_blocking(move || detail_with_pending(&entry, &snapshot, &service))
        .await
        .map_err(|error| format!("Unable to inspect project: {error}"))?
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
    let service = Arc::clone(&state.project_state);
    tauri::async_runtime::spawn_blocking(move || detail_with_pending(&entry, &snapshot, &service))
        .await
        .map_err(|error| format!("Unable to inspect project: {error}"))?
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
    let service = Arc::clone(&state.project_state);

    tauri::async_runtime::spawn_blocking(move || {
        git_sync.pull(&entry)?;
        detail_with_pending(&entry, &git_sync.snapshot(entry.id), &service)
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
    todo_id: String,
    input: CompleteTodoInput,
    state: State<'_, AppState>,
) -> Result<CompletionProposal, String> {
    let root = registered_root(id, &state)?;
    let service = Arc::clone(&state.project_state);
    tauri::async_runtime::spawn_blocking(move || service.complete_todo(id, &root, &todo_id, input))
        .await
        .map_err(|error| format!("Unable to request TODO completion: {error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_pending_reviews(
    id: Uuid,
    state: State<'_, AppState>,
) -> Result<Vec<CompletionProposal>, String> {
    registered_root(id, &state)?;
    state
        .project_state
        .pending_reviews(id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn approve_completion(
    id: Uuid,
    proposal_id: Uuid,
    state: State<'_, AppState>,
) -> Result<CompleteTodoResult, String> {
    let root = registered_root(id, &state)?;
    let service = Arc::clone(&state.project_state);
    tauri::async_runtime::spawn_blocking(move || service.approve_completion(id, &root, proposal_id))
        .await
        .map_err(|error| format!("Unable to approve completion: {error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn reject_completion(
    id: Uuid,
    proposal_id: Uuid,
    state: State<'_, AppState>,
) -> Result<CompletionProposal, String> {
    let service = Arc::clone(&state.project_state);
    tauri::async_runtime::spawn_blocking(move || service.reject_completion(id, proposal_id))
        .await
        .map_err(|error| format!("Unable to reject completion: {error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn add_work_history(
    id: Uuid,
    input: AddWorkHistoryInput,
    state: State<'_, AppState>,
) -> Result<CompletionProposal, String> {
    let root = registered_root(id, &state)?;
    let service = Arc::clone(&state.project_state);
    tauri::async_runtime::spawn_blocking(move || service.add_work_history(id, &root, input))
        .await
        .map_err(|error| format!("Unable to request working-history review: {error}"))?
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

fn detail_with_pending(
    entry: &models::RegistryEntry,
    snapshot: &git_sync::GitFetchSnapshot,
    service: &ProjectStateService,
) -> Result<ProjectDetail, String> {
    let mut detail = observer::detail(entry, snapshot);
    detail.state.pending_reviews = service
        .pending_reviews(entry.id)
        .map_err(|error| error.to_string())?;
    Ok(detail)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let registry_path = app.path().app_data_dir()?.join("registered-projects.json");
            let sync_cache_path = app.path().app_data_dir()?.join("git-sync-cache.json");
            let proposal_path = app.path().app_data_dir()?.join("completion-proposals.json");
            let subagent_settings_path = app.path().app_data_dir()?.join("subagent-settings.json");
            let codex_monitor_path = app.path().app_data_dir()?.join("codex-sessions.json");
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
            let project_state = Arc::new(
                ProjectStateService::load(proposal_path)
                    .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?,
            );
            let subagent_settings = SubagentSettingsStore::load(subagent_settings_path)
                .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            let codex_monitor = Arc::new(Mutex::new(
                CodexMonitorStore::load(codex_monitor_path)
                    .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?,
            ));
            agent_api::start(agent_api::AgentApiContext {
                registry: Arc::clone(&registry),
                project_state: Arc::clone(&project_state),
                codex_monitor: Arc::clone(&codex_monitor),
            })
            .map_err(Box::<dyn std::error::Error>::from)?;
            app.manage(AppState {
                registry,
                watcher: Mutex::new(watcher),
                git_sync: git_sync.clone(),
                project_state,
                subagent_settings: Mutex::new(subagent_settings),
                codex_monitor,
            });
            for entry in startup_entries {
                git_sync.fetch(app.handle().clone(), entry);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_projects,
            register_project,
            create_project,
            remove_project,
            open_project,
            refresh_project,
            refresh_projects,
            pull_project,
            add_todo,
            complete_todo,
            list_pending_reviews,
            approve_completion,
            reject_completion,
            add_work_history,
            get_subagent_settings,
            save_subagent_settings,
            reset_subagent_settings,
            preview_subagent_files,
            list_codex_sessions,
            link_codex_session,
            unlink_codex_session
        ])
        .run(tauri::generate_context!())
        .expect("error while running Projector");
}
