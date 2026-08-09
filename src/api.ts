import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import type {
  CodexMonitoringSnapshot,
  CompletionProposal,
  GeneratedSubagentFile,
  ProjectDetail,
  ProjectSummary,
  SettingsMigrationResult,
  SubagentSettings,
} from "./types";

export function listProjects(): Promise<ProjectSummary[]> {
  return invoke("list_projects");
}

export function registerProject(path: string): Promise<ProjectSummary> {
  return invoke("register_project", { path });
}

export function createProject(parentPath: string, name: string, initializeGit: boolean): Promise<ProjectSummary> {
  return invoke("create_project", { parentPath, name, initializeGit });
}

export function getSubagentSettings(): Promise<SubagentSettings> {
  return invoke("get_subagent_settings");
}

export function saveSubagentSettings(settings: SubagentSettings): Promise<SubagentSettings> {
  return invoke("save_subagent_settings", { settings });
}

export function resetSubagentSettings(): Promise<SubagentSettings> {
  return invoke("reset_subagent_settings");
}

export function previewSubagentFiles(settings: SubagentSettings): Promise<GeneratedSubagentFile[]> {
  return invoke("preview_subagent_files", { settings });
}

export function migrateProjectSettings(projectIds: string[]): Promise<SettingsMigrationResult[]> {
  return invoke("migrate_project_settings", { projectIds });
}

export function listCodexSessions(id: string): Promise<CodexMonitoringSnapshot> {
  return invoke("list_codex_sessions", { id });
}

export function linkCodexSession(id: string, sessionId: string): Promise<CodexMonitoringSnapshot> {
  return invoke("link_codex_session", { id, sessionId });
}

export function unlinkCodexSession(id: string, sessionId: string): Promise<CodexMonitoringSnapshot> {
  return invoke("unlink_codex_session", { id, sessionId });
}

export function removeProject(id: string): Promise<void> {
  return invoke("remove_project", { id });
}

export function openProject(id: string): Promise<ProjectDetail> {
  return invoke("open_project", { id });
}

export function refreshProject(id: string): Promise<ProjectDetail> {
  return invoke("refresh_project", { id });
}

export function refreshProjects(): Promise<ProjectSummary[]> {
  return invoke("refresh_projects");
}

export function pullProject(id: string): Promise<ProjectDetail> {
  return invoke("pull_project", { id });
}

export function approveCompletion(id: string, proposalId: string): Promise<void> {
  return invoke("approve_completion", { id, proposalId });
}

export function rejectCompletion(id: string, proposalId: string): Promise<CompletionProposal> {
  return invoke("reject_completion", { id, proposalId });
}

export function listPendingReviews(id: string): Promise<CompletionProposal[]> {
  return invoke("list_pending_reviews", { id });
}

export async function chooseProjectDirectory(): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    title: "Register a project directory",
  });
  return typeof selected === "string" ? selected : null;
}

export async function chooseProjectParentDirectory(): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    title: "Choose where to create the project",
  });
  return typeof selected === "string" ? selected : null;
}

export function onProjectChanged(callback: () => void): Promise<UnlistenFn> {
  return listen("project-changed", callback);
}

export function onGitSyncChanged(callback: (projectId: string) => void): Promise<UnlistenFn> {
  return listen<{ projectId: string }>("git-sync-changed", (event) => {
    callback(event.payload.projectId);
  });
}

