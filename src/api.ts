import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import type { ProjectDetail, ProjectSummary } from "./types";

export function listProjects(): Promise<ProjectSummary[]> {
  return invoke("list_projects");
}

export function registerProject(path: string): Promise<ProjectSummary> {
  return invoke("register_project", { path });
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

export async function chooseProjectDirectory(): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    title: "Register a project directory",
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

