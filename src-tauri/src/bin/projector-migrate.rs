use std::{fs, path::PathBuf};

use projector_lib::migration::{migrate_projects, report_markdown};

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let projector = manifest
        .parent()
        .expect("src-tauri must be inside the Projector repository");
    let projects = projector
        .parent()
        .expect("Projector must be directly inside the approved Projects directory");
    let report = migrate_projects(projects).unwrap_or_else(|error| {
        eprintln!("Projector migration failed: {error}");
        std::process::exit(1);
    });
    let report_path = projector.join("MIGRATION_REPORT.md");
    let no_changes = report.migrated_files.is_empty()
        && report.updated_agent_files.is_empty()
        && report.validation_failures.is_empty();
    if no_changes && report_path.exists() {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        return;
    }
    fs::write(&report_path, report_markdown(&report)).unwrap_or_else(|error| {
        eprintln!("Unable to write {}: {error}", report_path.display());
        std::process::exit(1);
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    if !report.validation_failures.is_empty() {
        std::process::exit(2);
    }
}
