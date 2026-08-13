import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import App, {
  DocumentPanel,
  CodexPanel,
  HistoryPanel,
  PendingReviewPanel,
  TodoPanel,
} from "./App";
import * as api from "./api";
import type { CodexMonitoringSnapshot, CompletionProposal, ProjectDetail, SubagentSettings } from "./types";

vi.mock("./api", () => ({
  listProjects: vi.fn(),
  registerProject: vi.fn(),
  createProject: vi.fn(),
  getSubagentSettings: vi.fn(),
  saveSubagentSettings: vi.fn(),
  resetSubagentSettings: vi.fn(),
  previewSubagentFiles: vi.fn(),
  migrateProjectSettings: vi.fn(),
  listCodexSessions: vi.fn(),
  linkCodexSession: vi.fn(),
  unlinkCodexSession: vi.fn(),
  removeProject: vi.fn(),
  openProject: vi.fn(),
  refreshProject: vi.fn(),
  refreshProjects: vi.fn(),
  pullProject: vi.fn(),
  openExternalUrl: vi.fn(),
  openProjectRoot: vi.fn(),
  approveCompletion: vi.fn(),
  rejectCompletion: vi.fn(),
  listPendingReviews: vi.fn().mockResolvedValue([]),
  chooseProjectDirectory: vi.fn(),
  chooseProjectParentDirectory: vi.fn(),
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
    startup: { name: "STARTUP.md", relativePath: "STARTUP.md", status: "available", content: "# Start locally\n\n```powershell\nnpm run tauri dev\n```", modifiedAt: null, truncated: false, error: null },
    todo: { name: "TODO.md", relativePath: null, status: "missing", content: null, modifiedAt: null, truncated: false, error: null },
    workingHistory: { name: "WORK_HISTORY.md", relativePath: null, status: "missing", content: null, modifiedAt: null, truncated: false, error: null },
  },
  state: {
    todos: {
      relativePath: null,
      items: [],
      warnings: [],
      preservedContent: null,
    },
    workingHistory: {
      relativePath: null,
      entries: [],
      categories: [],
      areas: [],
      warnings: [],
      preservedContent: null,
    },
    pendingReviews: [],
  },
};

const completionProposal: CompletionProposal = {
  id: "572031f1-d3f4-43a2-89dd-2e48d2fb4376",
  projectId: detail.project.id,
  requestedAt: "2026-07-29T14:15:00Z",
  kind: "todoCompletion",
  todo: {
    id: "TODO-001",
    title: "Review completion",
    priority: "high",
    category: "feature",
    area: "project-state",
    dependencies: [],
    rationale: "Users own completion.",
    acceptanceCriteria: "Approval is required.",
  },
  proposedEntry: {
    occurredAt: "2026-07-29T10:15:00",
    title: "Review completion",
    category: "feature",
    area: "project-state",
    summary: "Added the review workflow.",
    limitations: "none",
  },
};

const workHistoryProposal: CompletionProposal = {
  id: "9cc169ab-ac65-4b2b-83a7-d91140ea45a2",
  projectId: detail.project.id,
  requestedAt: "2026-07-30T14:15:00Z",
  kind: "workHistory",
  todo: null,
  proposedEntry: {
    occurredAt: "2026-07-30T10:15:00",
    title: "Documented standalone work",
    category: "documentation",
    area: "project-state",
    summary: "Prepared a standalone history entry for review.",
    limitations: "none",
  },
};

const subagentSettings: SubagentSettings = {
  version: 1,
  projectorSection: "## Projector\n\nUse the local Projector API.",
  subagentsSection: "## Subagents\n\nUse the lowest capable worker tier.",
  workerLow: {
    fileName: "worker-low.toml",
    name: "worker_low",
    description: "Handles small changes.",
    model: "gpt-5.6-luna",
    modelReasoningEffort: "medium",
    sandboxMode: "workspace-write",
    developerInstructions: "Implement small changes and run focused tests.",
  },
  workerMedium: {
    fileName: "worker-medium.toml",
    name: "worker_medium",
    description: "Handles standard changes.",
    model: "gpt-5.6-luna",
    modelReasoningEffort: "max",
    sandboxMode: "workspace-write",
    developerInstructions: "Investigate, implement, and test standard changes.",
  },
  workerHigh: {
    fileName: "worker-high.toml",
    name: "worker_high",
    description: "Handles complex changes.",
    model: "gpt-5.6-sol",
    modelReasoningEffort: "high",
    sandboxMode: "workspace-write",
    developerInstructions: "Trace coupled behavior and run full validation.",
  },
};

const detectedCodexMonitoring: CodexMonitoringSnapshot = {
  detectedSessions: [{
    sessionId: "session-detected",
    cwd: "C:\\code\\example",
    linkedProjectId: null,
    state: "running",
    firstSeenAt: "2026-08-03T12:00:00Z",
    lastSeenAt: "2026-08-03T12:01:00Z",
    agents: [],
    transitions: [{
      kind: "sessionStarted",
      agentId: null,
      agentType: null,
      observedAt: "2026-08-03T12:00:00Z",
    }],
  }],
  linkedSessions: [],
};

const linkedCodexMonitoring: CodexMonitoringSnapshot = {
  detectedSessions: [],
  linkedSessions: [{
    ...detectedCodexMonitoring.detectedSessions[0],
    linkedProjectId: detail.project.id,
    agents: [{
      agentId: "agent-1",
      agentType: "worker_low",
      state: "stopped",
      firstSeenAt: "2026-08-03T12:00:10Z",
      lastSeenAt: "2026-08-03T12:00:30Z",
    }, {
      agentId: "agent-2",
      agentType: "worker_medium",
      state: "unknown",
      firstSeenAt: "2026-08-03T12:00:20Z",
      lastSeenAt: "2026-08-03T12:01:00Z",
    }],
    transitions: [{
      kind: "subagentStopped",
      agentId: "agent-1",
      agentType: "worker_low",
      observedAt: "2026-08-03T12:00:30Z",
    }, {
      kind: "subagentUnknown",
      agentId: "agent-2",
      agentType: "worker_medium",
      observedAt: "2026-08-03T12:01:00Z",
    }],
  }],
};

describe("App", () => {
  beforeEach(() => {
    vi.mocked(api.listProjects).mockReset();
    vi.mocked(api.openProject).mockReset();
    vi.mocked(api.refreshProject).mockReset();
    vi.mocked(api.refreshProjects).mockReset();
    vi.mocked(api.pullProject).mockReset();
    vi.mocked(api.openProjectRoot).mockReset();
    vi.mocked(api.approveCompletion).mockReset();
    vi.mocked(api.rejectCompletion).mockReset();
    vi.mocked(api.listPendingReviews).mockReset();
    vi.mocked(api.createProject).mockReset();
    vi.mocked(api.getSubagentSettings).mockReset();
    vi.mocked(api.saveSubagentSettings).mockReset();
    vi.mocked(api.resetSubagentSettings).mockReset();
    vi.mocked(api.previewSubagentFiles).mockReset();
    vi.mocked(api.migrateProjectSettings).mockReset();
    vi.mocked(api.listCodexSessions).mockReset();
    vi.mocked(api.linkCodexSession).mockReset();
    vi.mocked(api.unlinkCodexSession).mockReset();
    vi.mocked(api.listProjects).mockResolvedValue([]);
    vi.mocked(api.refreshProjects).mockResolvedValue([]);
    vi.mocked(api.listPendingReviews).mockResolvedValue([]);
    vi.mocked(api.getSubagentSettings).mockResolvedValue(subagentSettings);
    vi.mocked(api.saveSubagentSettings).mockImplementation(async (settings) => settings);
    vi.mocked(api.resetSubagentSettings).mockResolvedValue(subagentSettings);
    vi.mocked(api.previewSubagentFiles).mockResolvedValue([
      { path: "AGENTS.md", content: "# AGENTS.md\n\n## Subagents" },
      { path: ".codex/agents/worker-low.toml", content: 'name = "worker_low"' },
      { path: ".codex/agents/worker-medium.toml", content: 'name = "worker_medium"' },
      { path: ".codex/agents/worker-high.toml", content: 'name = "worker_high"' },
    ]);
    vi.mocked(api.migrateProjectSettings).mockResolvedValue([]);
    vi.mocked(api.listCodexSessions).mockResolvedValue({ detectedSessions: [], linkedSessions: [] });
    vi.mocked(api.linkCodexSession).mockResolvedValue(linkedCodexMonitoring);
    vi.mocked(api.unlinkCodexSession).mockResolvedValue(detectedCodexMonitoring);
  });

  it("shows a clear first-run state", async () => {
    render(<App />);
    expect(await screen.findByText("Your projects, at a glance")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create your first project" })).toBeInTheDocument();
  });

  it("opens a registered project and exposes its information tabs", async () => {
    vi.mocked(api.listProjects).mockResolvedValue([detail.project]);
    vi.mocked(api.openProject).mockResolvedValue(detail);
    render(<App />);

    expect(await screen.findByRole("heading", { name: "Example" })).toBeInTheDocument();
    expect(screen.getAllByText("main")).toHaveLength(2);
    await userEvent.click(screen.getByRole("tab", { name: "Startup" }));
    expect(screen.getByRole("heading", { name: "Start locally" })).toBeInTheDocument();
    expect(screen.getByText("npm run tauri dev")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("tab", { name: "TODO" }));
    expect(screen.getByText("TODO.md was not found")).toBeInTheDocument();
  });

  it("opens a registered project's root folder from its sidebar panel", async () => {
    vi.mocked(api.listProjects).mockResolvedValue([detail.project]);
    vi.mocked(api.openProject).mockResolvedValue(detail);
    vi.mocked(api.openProjectRoot).mockResolvedValue(undefined);
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Open Example folder" }));

    expect(api.openProjectRoot).toHaveBeenCalledWith(detail.project.id);
  });

  it("registers a selected directory", async () => {
    vi.mocked(api.chooseProjectDirectory).mockResolvedValue("C:\\code\\example");
    vi.mocked(api.registerProject).mockResolvedValue(detail.project);
    vi.mocked(api.openProject).mockResolvedValue(detail);
    vi.mocked(api.listProjects)
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([detail.project]);
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "Register an existing folder" }));
    await waitFor(() => expect(api.registerProject).toHaveBeenCalledWith("C:\\code\\example"));
    expect(await screen.findByRole("heading", { name: "Example" })).toBeInTheDocument();
  });

  it("creates and opens a Projector-ready project", async () => {
    vi.mocked(api.chooseProjectParentDirectory).mockResolvedValue("C:\\code");
    vi.mocked(api.createProject).mockResolvedValue(detail.project);
    vi.mocked(api.openProject).mockResolvedValue(detail);
    vi.mocked(api.listProjects)
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([detail.project]);
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "Create your first project" }));
    expect(screen.getByRole("checkbox", { name: "Initialize a Git repository" })).toBeChecked();
    await userEvent.type(screen.getByLabelText("Project name"), "Example");
    await userEvent.click(screen.getByRole("button", { name: "Choose…" }));
    await waitFor(() => expect(screen.getByLabelText("Parent folder")).toHaveValue("C:\\code"));
    await userEvent.click(screen.getByRole("button", { name: "Confirm project creation" }));

    await waitFor(() => expect(api.createProject).toHaveBeenCalledWith("C:\\code", "Example", true));
    expect(await screen.findByRole("heading", { name: "Example" })).toBeInTheDocument();
  });

  it("can create a project without initializing Git", async () => {
    vi.mocked(api.chooseProjectParentDirectory).mockResolvedValue("C:\\code");
    vi.mocked(api.createProject).mockResolvedValue(detail.project);
    vi.mocked(api.openProject).mockResolvedValue(detail);
    vi.mocked(api.listProjects)
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([detail.project]);
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "Create your first project" }));
    await userEvent.type(screen.getByLabelText("Project name"), "Example");
    await userEvent.click(screen.getByRole("button", { name: "Choose…" }));
    await userEvent.click(screen.getByRole("checkbox", { name: "Initialize a Git repository" }));
    await userEvent.click(screen.getByRole("button", { name: "Confirm project creation" }));

    await waitFor(() => expect(api.createProject).toHaveBeenCalledWith("C:\\code", "Example", false));
  });

  it("edits and saves separate Projector and subagent settings", async () => {
    render(<App />);

    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(await screen.findByRole("heading", { name: "Project settings" })).toBeInTheDocument();
    expect(screen.getByLabelText("AGENTS.md Projector section")).toHaveValue(subagentSettings.projectorSection);
    expect(screen.getByLabelText("Worker low model")).toHaveValue("gpt-5.6-luna");
    expect(screen.getByLabelText("Worker low reasoning effort")).toHaveValue("medium");
    expect(screen.getByLabelText("Worker medium reasoning effort")).toHaveValue("max");

    await userEvent.clear(screen.getByLabelText("Worker low description"));
    await userEvent.type(screen.getByLabelText("Worker low description"), "Edited low worker.");
    await userEvent.selectOptions(screen.getByLabelText("Worker high reasoning effort"), "xhigh");
    await userEvent.clear(screen.getByLabelText("AGENTS.md Projector section"));
    await userEvent.type(screen.getByLabelText("AGENTS.md Projector section"), "## Projector\n\nEdited API guidance.");
    await userEvent.click(screen.getByRole("button", { name: "Save settings" }));

    await waitFor(() => expect(api.saveSubagentSettings).toHaveBeenCalledWith(expect.objectContaining({
      projectorSection: "## Projector\n\nEdited API guidance.",
      workerLow: expect.objectContaining({ description: "Edited low worker." }),
      workerHigh: expect.objectContaining({ modelReasoningEffort: "xhigh" }),
    })));
    expect(await screen.findByRole("status")).toHaveTextContent("Project and subagent settings saved.");
  });

  it("previews generated files and resets bundled defaults", async () => {
    render(<App />);

    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", { name: "Project settings" });
    await userEvent.click(screen.getByRole("button", { name: "Preview generated files" }));

    const preview = await screen.findByLabelText("Generated file preview");
    expect(within(preview).getByText(".codex/agents/worker-medium.toml")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Reset to bundled defaults" }));
    await waitFor(() => expect(api.resetSubagentSettings).toHaveBeenCalledOnce());
    expect(await screen.findByRole("status")).toHaveTextContent("Bundled Projector and subagent defaults restored.");
  });

  it("saves and migrates selected registered projects", async () => {
    vi.mocked(api.listProjects).mockResolvedValue([detail.project]);
    vi.mocked(api.openProject).mockResolvedValue(detail);
    vi.mocked(api.migrateProjectSettings).mockResolvedValue([{
      projectId: detail.project.id,
      projectName: detail.project.name,
      updatedFiles: ["AGENTS.md", ".codex/agents/worker-low.toml"],
      error: null,
    }]);
    render(<App />);

    await screen.findByRole("heading", { name: "Example" });
    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", { name: "Project settings" });
    await userEvent.click(screen.getByRole("checkbox", { name: /Example/ }));
    await userEvent.click(screen.getByRole("button", { name: "Save and migrate selected" }));

    await waitFor(() => expect(api.saveSubagentSettings).toHaveBeenCalledWith(subagentSettings));
    expect(api.migrateProjectSettings).toHaveBeenCalledWith([detail.project.id]);
    expect(await screen.findByRole("status")).toHaveTextContent("Migrated 1 of 1 selected project; 1 had file changes.");
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

  it("manually links a detected Codex session from the project tab", async () => {
    vi.mocked(api.listProjects).mockResolvedValue([detail.project]);
    vi.mocked(api.openProject).mockResolvedValue(detail);
    vi.mocked(api.listCodexSessions).mockResolvedValue(detectedCodexMonitoring);
    render(<App />);

    await screen.findByRole("heading", { name: "Example" });
    await userEvent.click(screen.getByRole("tab", { name: "Codex" }));
    expect(await screen.findByLabelText("Detected Codex session")).toHaveValue("session-detected");
    await userEvent.click(screen.getByRole("button", { name: "Link session" }));
    await waitFor(() => expect(api.linkCodexSession).toHaveBeenCalledWith(
      detail.project.id,
      "session-detected",
    ));
    expect(await screen.findByText("worker_low")).toBeInTheDocument();
  });

  it("surfaces Codex monitoring API failures", async () => {
    vi.mocked(api.listProjects).mockResolvedValue([detail.project]);
    vi.mocked(api.openProject).mockResolvedValue(detail);
    vi.mocked(api.listCodexSessions).mockRejectedValue(new Error("monitor offline"));
    render(<App />);

    await screen.findByRole("heading", { name: "Example" });
    await userEvent.click(screen.getByRole("tab", { name: "Codex" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("monitor offline");
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

  it("approves a pending proposal through the local desktop command", async () => {
    const pendingDetail = {
      ...detail,
      state: { ...detail.state, pendingReviews: [completionProposal] },
    };
    vi.mocked(api.listProjects).mockResolvedValue([detail.project]);
    vi.mocked(api.openProject).mockResolvedValue(pendingDetail);
    vi.mocked(api.listPendingReviews).mockResolvedValue([completionProposal]);
    vi.mocked(api.approveCompletion).mockResolvedValue();
    vi.mocked(api.refreshProject).mockResolvedValue(detail);
    render(<App />);

    await screen.findByRole("heading", { name: "Example" });
    await userEvent.click(screen.getByRole("tab", { name: /Pending Review/ }));
    await userEvent.click(screen.getByRole("button", { name: "Approve" }));

    await waitFor(() => {
      expect(api.approveCompletion).toHaveBeenCalledWith(
        detail.project.id,
        completionProposal.id,
      );
    });
    expect(api.refreshProject).toHaveBeenCalledWith(detail.project.id);
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

  it("copies PowerShell startup scripts", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    render(<DocumentPanel copyPowerShell document={detail.documents.startup} />);

    await userEvent.click(screen.getByRole("button", { name: "Copy" }));

    expect(writeText).toHaveBeenCalledWith("npm run tauri dev");
    expect(screen.getByRole("button", { name: "Copied" })).toBeInTheDocument();
  });

  it("opens Startup web links through the external-browser command", async () => {
    vi.mocked(api.openExternalUrl).mockResolvedValue(undefined);
    render(
      <DocumentPanel
        copyPowerShell
        document={{
          ...detail.documents.startup,
          content: "Open [the local site](http://127.0.0.1:4817).",
        }}
      />,
    );

    await userEvent.click(screen.getByRole("link", { name: "the local site" }));

    expect(api.openExternalUrl).toHaveBeenCalledWith("http://127.0.0.1:4817");
  });
});

describe("CodexPanel", () => {
  it("shows factual subagent states and can unlink a session", async () => {
    const onUnlink = vi.fn();
    render(
      <CodexPanel
        busySession={null}
        monitoring={linkedCodexMonitoring}
        onLink={vi.fn()}
        onUnlink={onUnlink}
      />,
    );

    expect(screen.getByText("worker_low")).toBeInTheDocument();
    expect(screen.getByText("worker_medium")).toBeInTheDocument();
    expect(screen.getByText("stopped")).toBeInTheDocument();
    expect(screen.getByText("unknown")).toBeInTheDocument();
    expect(screen.getByText("worker_medium subagent state became unknown")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Unlink" }));
    expect(onUnlink).toHaveBeenCalledWith("session-detected");
  });

  it("shows the manual setup empty state without implying automatic linking", () => {
    render(
      <CodexPanel
        busySession={null}
        monitoring={{ detectedSessions: [], linkedSessions: [] }}
        onLink={vi.fn()}
        onUnlink={vi.fn()}
      />,
    );
    expect(screen.getByText(/No unlinked sessions detected/)).toBeInTheDocument();
    expect(screen.getByText("No Codex session linked")).toBeInTheDocument();
  });
});

describe("structured project state", () => {
  it("shows both proposal kinds and exposes local approve and reject actions", async () => {
    const onApprove = vi.fn();
    const onReject = vi.fn();
    render(
      <PendingReviewPanel
        proposals={[completionProposal, workHistoryProposal]}
        reviewingProposal={null}
        onApprove={onApprove}
        onReject={onReject}
      />,
    );

    expect(screen.getByRole("heading", { name: "Review completion" })).toBeInTheDocument();
    expect(screen.getByText("Added the review workflow.")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Documented standalone work" })).toBeInTheDocument();
    expect(screen.getByText("Prepared a standalone history entry for review.")).toBeInTheDocument();
    expect(screen.getByText("Working history")).toBeInTheDocument();

    const approveButtons = screen.getAllByRole("button", { name: "Approve" });
    await userEvent.click(approveButtons[0]);
    expect(onApprove).toHaveBeenCalledWith("572031f1-d3f4-43a2-89dd-2e48d2fb4376");
    await userEvent.click(approveButtons[1]);
    expect(onApprove).toHaveBeenCalledWith("9cc169ab-ac65-4b2b-83a7-d91140ea45a2");

    const rejectButtons = screen.getAllByRole("button", { name: "Reject" });
    await userEvent.click(rejectButtons[1]);
    expect(onReject).toHaveBeenCalledWith("9cc169ab-ac65-4b2b-83a7-d91140ea45a2");
  });

  it("shows four compact priority boxes and opens TODO details in a closable window", async () => {
    render(
      <TodoPanel
        source={{ ...detail.documents.todo, status: "available", relativePath: "TODO.md" }}
        document={{
          relativePath: "TODO.md",
          items: [
            {
              id: "TODO-001",
              title: "Ship state management",
              priority: "critical",
              category: "feature",
              area: "project-state",
              dependencies: ["TODO-999"],
              rationale: "Agents need one safe contract.",
              acceptanceCriteria: "The parser and writer agree.",
            },
          ],
          warnings: [
            {
              code: "missing_dependency",
              message: "Dependency TODO-999 does not exist in this project.",
              itemId: "TODO-001",
            },
          ],
          preservedContent: "Legacy note retained.",
        }}
      />,
    );

    expect(screen.getByRole("button", { name: "Open Ship state management" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "critical" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "high" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "medium" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "low" })).toBeInTheDocument();
    expect(screen.getByText("blocked")).toBeInTheDocument();
    expect(screen.queryByText("Agents need one safe contract.")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("TODO dependency relationships")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Open Ship state management" }));
    const dialog = screen.getByRole("dialog", { name: "Ship state management" });
    expect(dialog).toBeInTheDocument();
    expect(within(dialog).getByLabelText("TODO dependencies")).toHaveTextContent("TODO-999");
    expect(screen.getByText("Agents need one safe contract.")).toBeInTheDocument();
    expect(screen.getByText("The parser and writer agree.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Close details" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("TODO-999");
    expect(screen.getByText("Preserved unrecognized source content")).toBeInTheDocument();
  });

  it("filters TODOs by category while preserving the priority columns", async () => {
    render(
      <TodoPanel
        source={{ ...detail.documents.todo, status: "available", relativePath: "TODO.md" }}
        document={{
          relativePath: "TODO.md",
          items: [
            {
              id: "TODO-001",
              title: "Build the feature",
              priority: "high",
              category: "feature",
              area: "ui",
              dependencies: [],
              rationale: "Needed.",
              acceptanceCriteria: "It works.",
            },
            {
              id: "TODO-002",
              title: "Research the workflow",
              priority: "low",
              category: "research",
              area: "product",
              dependencies: [],
              rationale: "Clarify the design.",
              acceptanceCriteria: "The decision is documented.",
            },
          ],
          warnings: [],
          preservedContent: null,
        }}
      />,
    );

    const category = screen.getByLabelText("Category");
    expect(category).toHaveValue("all");
    expect(screen.getByRole("button", { name: "Open Build the feature" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open Research the workflow" })).toBeInTheDocument();

    await userEvent.selectOptions(category, "research");
    expect(screen.queryByRole("button", { name: "Open Build the feature" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open Research the workflow" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "critical" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "low" })).toBeInTheDocument();

    await userEvent.selectOptions(category, "all");
    expect(screen.getByRole("button", { name: "Open Build the feature" })).toBeInTheDocument();
  });

  it("shows history metadata newest first and opens details in a closable window", async () => {
    render(
      <HistoryPanel
        source={{ ...detail.documents.workingHistory, status: "available", relativePath: "WORK_HISTORY.md" }}
        document={{
          relativePath: "WORK_HISTORY.md",
          categories: ["feature", "bugfix"],
          areas: ["project-state", "ui"],
          warnings: [],
          preservedContent: null,
          entries: [
            {
              occurredAt: "2026-07-23T16:30:00",
              title: "State API implemented",
              category: "feature",
              area: "project-state",
              summary: "Added the API.",
              limitations: "none",
            },
            {
              occurredAt: "2026-07-23T17:00:00",
              title: "UI fixed",
              category: "bugfix",
              area: "ui",
              summary: "Fixed the UI.",
              limitations: "none",
            },
          ],
        }}
      />,
    );

    const historyEntries = screen.getAllByRole("button", { name: /^Open / });
    expect(historyEntries.map((button) => button.getAttribute("aria-label"))).toEqual([
      "Open UI fixed",
      "Open State API implemented",
    ]);
    const visibleMetadata = Array.from(document.querySelectorAll(".history-row-metadata"));
    expect(visibleMetadata[0]).toHaveTextContent("2026");
    expect(visibleMetadata[0]).toHaveTextContent("bugfix");
    expect(visibleMetadata[0]).toHaveTextContent("ui");
    expect(visibleMetadata[1]).toHaveTextContent("feature");
    expect(visibleMetadata[1]).toHaveTextContent("project-state");
    expect(screen.queryByText("Added the API.")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Open State API implemented" }));
    expect(screen.getByRole("dialog", { name: "State API implemented" })).toBeInTheDocument();
    expect(screen.getByText("Added the API.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Close details" }));

    await userEvent.selectOptions(screen.getByLabelText("Category"), "feature");
    expect(screen.getByRole("button", { name: "Open State API implemented" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open UI fixed" })).not.toBeInTheDocument();
  });
});
