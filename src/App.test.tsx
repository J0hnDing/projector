import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import App, { DocumentPanel } from "./App";
import * as api from "./api";
import type { ProjectDetail } from "./types";

vi.mock("./api", () => ({
  listProjects: vi.fn(),
  registerProject: vi.fn(),
  removeProject: vi.fn(),
  openProject: vi.fn(),
  refreshProject: vi.fn(),
  refreshProjects: vi.fn(),
  pullProject: vi.fn(),
  chooseProjectDirectory: vi.fn(),
  onProjectChanged: vi.fn().mockResolvedValue(() => undefined),
  onGitSyncChanged: vi.fn().mockResolvedValue(() => undefined),
}));

const detail: ProjectDetail = {
  project: {
    id: "9ef6a0ed-b221-4a42-b05e-fc19860261b4",
    name: "Example",
    path: "C:\\code\\example",
    registeredAt: "2026-07-22T10:00:00Z",
    lastOpened: "2026-07-22T11:00:00Z",
    git: {
      isRepository: true,
      branch: "main",
      dirty: false,
      recentCommits: [],
      lastActivity: "2026-07-22T10:30:00Z",
      upstream: "origin/main",
      ahead: 0,
      behind: 0,
      syncStatus: "synchronized",
      syncMessage: null,
      fetchStatus: "succeeded",
      lastSuccessfulFetch: "2026-07-22T10:31:00Z",
      fetchError: null,
      error: null,
    },
  },
  documents: {
    readme: { name: "README.md", relativePath: "README.md", status: "available", content: "# Hello", modifiedAt: null, truncated: false, error: null },
    todo: { name: "TODO.md", relativePath: null, status: "missing", content: null, modifiedAt: null, truncated: false, error: null },
    workingHistory: { name: "WORK_HISTORY.md", relativePath: null, status: "missing", content: null, modifiedAt: null, truncated: false, error: null },
  },
};

describe("App", () => {
  beforeEach(() => {
    vi.mocked(api.listProjects).mockReset();
    vi.mocked(api.openProject).mockReset();
    vi.mocked(api.refreshProject).mockReset();
    vi.mocked(api.refreshProjects).mockReset();
    vi.mocked(api.pullProject).mockReset();
    vi.mocked(api.listProjects).mockResolvedValue([]);
    vi.mocked(api.refreshProjects).mockResolvedValue([]);
  });

  it("shows a clear first-run state", async () => {
    render(<App />);
    expect(await screen.findByText("Your projects, at a glance")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Register your first project" })).toBeInTheDocument();
  });

  it("opens a registered project and exposes its information tabs", async () => {
    vi.mocked(api.listProjects).mockResolvedValue([detail.project]);
    vi.mocked(api.openProject).mockResolvedValue(detail);
    render(<App />);

    expect(await screen.findByRole("heading", { name: "Example" })).toBeInTheDocument();
    expect(screen.getAllByText("main")).toHaveLength(2);
    await userEvent.click(screen.getByRole("tab", { name: "TODO" }));
    expect(screen.getByText("TODO.md was not found")).toBeInTheDocument();
  });

  it("registers a selected directory", async () => {
    vi.mocked(api.chooseProjectDirectory).mockResolvedValue("C:\\code\\example");
    vi.mocked(api.registerProject).mockResolvedValue(detail.project);
    vi.mocked(api.openProject).mockResolvedValue(detail);
    vi.mocked(api.listProjects)
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([detail.project]);
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "Register your first project" }));
    await waitFor(() => expect(api.registerProject).toHaveBeenCalledWith("C:\\code\\example"));
    expect(await screen.findByRole("heading", { name: "Example" })).toBeInTheDocument();
  });

  it("starts background Git synchronization on manual refresh", async () => {
    vi.mocked(api.listProjects).mockResolvedValue([detail.project]);
    vi.mocked(api.openProject).mockResolvedValue(detail);
    vi.mocked(api.refreshProjects).mockResolvedValue([
      { ...detail.project, git: { ...detail.project.git, fetchStatus: "fetching" } },
    ]);
    vi.mocked(api.refreshProject).mockResolvedValue({
      ...detail,
      project: { ...detail.project, git: { ...detail.project.git, fetchStatus: "fetching" } },
    });
    render(<App />);

    await screen.findByRole("heading", { name: "Example" });
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(api.refreshProjects).toHaveBeenCalledOnce());
  });

  it("manually pulls the selected project", async () => {
    vi.mocked(api.listProjects).mockResolvedValue([detail.project]);
    vi.mocked(api.openProject).mockResolvedValue(detail);
    vi.mocked(api.pullProject).mockResolvedValue(detail);
    render(<App />);

    await screen.findByRole("heading", { name: "Example" });
    await userEvent.click(screen.getByRole("button", { name: "Pull" }));
    await waitFor(() => expect(api.pullProject).toHaveBeenCalledWith(detail.project.id));
  });

  it("disables pull when the working tree is dirty", async () => {
    const dirtyDetail = {
      ...detail,
      project: {
        ...detail.project,
        git: { ...detail.project.git, dirty: true },
      },
    };
    vi.mocked(api.listProjects).mockResolvedValue([dirtyDetail.project]);
    vi.mocked(api.openProject).mockResolvedValue(dirtyDetail);
    render(<App />);

    const pull = await screen.findByRole("button", { name: "Pull" });
    expect(pull).toBeDisabled();
    expect(pull).toHaveAttribute("title", "Pull requires a clean working tree");
  });
});

describe("DocumentPanel", () => {
  it("renders Markdown and reports truncation", () => {
    render(<DocumentPanel document={{ ...detail.documents.readme, content: "# Rendered title", truncated: true }} />);
    expect(screen.getByRole("heading", { name: "Rendered title" })).toBeInTheDocument();
    expect(screen.getByText(/larger than 2 MB/i)).toBeInTheDocument();
  });

  it("renders safe README HTML without exposing unsafe elements", () => {
    render(
      <DocumentPanel
        document={{
          ...detail.documents.readme,
          content: '<div align="center"><strong>Centered content</strong></div><script>alert("unsafe")</script>',
        }}
      />,
    );

    const content = screen.getByText("Centered content");
    expect(content.closest("div")).toHaveAttribute("align", "center");
    expect(document.querySelector("script")).not.toBeInTheDocument();
    expect(screen.queryByText(/unsafe/)).not.toBeInTheDocument();
  });
});
