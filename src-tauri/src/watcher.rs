use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectChangedEvent {
    paths: Vec<PathBuf>,
}

pub struct ProjectWatcher {
    watcher: RecommendedWatcher,
    roots: HashSet<PathBuf>,
}

impl ProjectWatcher {
    pub fn new(app: AppHandle) -> Result<Self, notify::Error> {
        let watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            let Ok(event) = result else { return };
            if matches!(event.kind, EventKind::Access(_)) {
                return;
            }
            let _ = app.emit(
                "project-changed",
                ProjectChangedEvent { paths: event.paths },
            );
        })?;
        Ok(Self {
            watcher,
            roots: HashSet::new(),
        })
    }

    pub fn watch(&mut self, root: &Path) -> Result<(), notify::Error> {
        if self.roots.insert(root.to_path_buf()) {
            self.watcher.watch(root, RecursiveMode::Recursive)?;
        }
        Ok(())
    }

    pub fn unwatch(&mut self, root: &Path) {
        if self.roots.remove(root) {
            let _ = self.watcher.unwatch(root);
        }
    }
}
