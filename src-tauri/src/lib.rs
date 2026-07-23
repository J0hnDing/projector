mod models;
mod observer;
mod registry;
mod watcher;

use std::{path::PathBuf, sync::Mutex};

use models::{ProjectDetail, ProjectSummary};
use registry::RegistryStore;
use tauri::{Manager, State};
use uuid::Uuid;
use watcher::ProjectWatcher;

struct AppState {
    registry: Mutex<RegistryStore>,
    watcher: Mutex<ProjectWatcher>,
}

#[tauri::command]
async fn list_projects(state: State<'_, AppState>) -> Result<Vec<ProjectSummary>, String> {
    let entries = state
        .registry
        .lock()
        .map_err(|_| "The project registry is unavailable".to_string())?
        .entries()
        .to_vec();

    tauri::async_runtime::spawn_blocking(move || {
        let mut projects: Vec<_> = entries.iter().map(observer::summarize).collect();
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

    tauri::async_runtime::spawn_blocking(move || observer::summarize(&entry))
        .await
        .map_err(|error| format!("Unable to inspect project: {error}"))
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

    tauri::async_runtime::spawn_blocking(move || observer::detail(&entry))
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

    tauri::async_runtime::spawn_blocking(move || observer::detail(&entry))
        .await
        .map_err(|error| format!("Unable to inspect project: {error}"))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let registry_path = app.path().app_data_dir()?.join("registered-projects.json");
            let registry = RegistryStore::load(registry_path)
                .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            let mut watcher = ProjectWatcher::new(app.handle().clone())?;
            for entry in registry.entries() {
                if entry.path.is_dir() {
                    let _ = watcher.watch(&entry.path);
                }
            }
            app.manage(AppState {
                registry: Mutex::new(registry),
                watcher: Mutex::new(watcher),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_projects,
            register_project,
            remove_project,
            open_project,
            refresh_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running Projector");
}
