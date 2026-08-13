import { isValidElement, useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { FormEvent, MouseEvent, ReactNode } from "react";
import rehypeRaw from "rehype-raw";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";

import * as api from "./api";
import type {
  CodexLifecycleState,
  CodexMonitoringSnapshot,
  CodexTransitionKind,
  CompletionProposal,
  GeneratedSubagentFile,
  GitInfo,
  ProjectDetail,
  ProjectDocument,
  ProjectSummary,
  ReasoningEffort,
  SubagentSettings,
  TodoDocument,
  TodoPriority,
  ValidationWarning,
  WorkCategory,
  WorkHistoryDocument,
  WorkHistoryEntry,
} from "./types";

type Tab = "overview" | "readme" | "startup" | "todo" | "pending" | "history" | "git" | "codex";

const tabs: Array<{ id: Tab; label: string }> = [
  { id: "overview", label: "Overview" },
  { id: "readme", label: "README" },
  { id: "startup", label: "Startup" },
  { id: "todo", label: "TODO" },
  { id: "pending", label: "Pending Review" },
  { id: "history", label: "Working history" },
  { id: "git", label: "Git activity" },
  { id: "codex", label: "Codex" },
];

type WorkerKey = "workerLow" | "workerMedium" | "workerHigh";

const workerDefinitions: Array<{ key: WorkerKey; label: string }> = [
  { key: "workerLow", label: "Worker low" },
  { key: "workerMedium", label: "Worker medium" },
  { key: "workerHigh", label: "Worker high" },
];

const reasoningEfforts: ReasoningEffort[] = ["low", "medium", "high", "xhigh", "max", "ultra"];

const workCategories: WorkCategory[] = [
  "feature",
  "bugfix",
  "refactor",
  "test",
  "documentation",
  "research",
  "others",
];

const markdownSanitizeSchema = {
  ...defaultSchema,
  attributes: {
    ...defaultSchema.attributes,
    div: [...(defaultSchema.attributes?.div ?? []), "align"],
  },
};

export default function App() {
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [detail, setDetail] = useState<ProjectDetail | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("overview");
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [pulling, setPulling] = useState(false);
  const [reviewingProposal, setReviewingProposal] = useState<string | null>(null);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [creatingProject, setCreatingProject] = useState(false);
  const [newProjectName, setNewProjectName] = useState("");
  const [newProjectParent, setNewProjectParent] = useState("");
  const [initializeGit, setInitializeGit] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsLoading, setSettingsLoading] = useState(false);
  const [settingsSaving, setSettingsSaving] = useState(false);
  const [settingsMigrating, setSettingsMigrating] = useState(false);
  const [migrationProjectIds, setMigrationProjectIds] = useState<string[]>([]);
  const [subagentSettings, setSubagentSettings] = useState<SubagentSettings | null>(null);
  const [settingsPreview, setSettingsPreview] = useState<GeneratedSubagentFile[] | null>(null);
  const [settingsMessage, setSettingsMessage] = useState<string | null>(null);
  const [codexMonitoring, setCodexMonitoring] = useState<CodexMonitoringSnapshot | null>(null);
  const [codexBusySession, setCodexBusySession] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const selectedIdRef = useRef<string | null>(null);
  const refreshTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const createNameInput = useRef<HTMLInputElement>(null);

  useEffect(() => {
    selectedIdRef.current = selectedId;
  }, [selectedId]);

  const openProject = useCallback(async (id: string, updateOpened = true, showProgress = true) => {
    setSelectedId(id);
    selectedIdRef.current = id;
    setError(null);
    if (showProgress) setRefreshing(true);
    try {
      const nextDetail = updateOpened
        ? await api.openProject(id)
        : await api.refreshProject(id);
      setDetail(nextDetail);
    } catch (reason) {
      setDetail(null);
      setError(messageFrom(reason));
    } finally {
      if (showProgress) setRefreshing(false);
    }
  }, []);

  const loadProjects = useCallback(async () => {
    const nextProjects = await api.listProjects();
    setProjects(nextProjects);
    return nextProjects;
  }, []);

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const nextProjects = await api.listProjects();
        if (!active) return;
        setProjects(nextProjects);
        if (nextProjects[0]) {
          await openProject(nextProjects[0].id);
        }
      } catch (reason) {
        if (active) setError(messageFrom(reason));
      } finally {
        if (active) setLoading(false);
      }
    })();
    return () => {
      active = false;
    };
  }, [openProject]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void api.onProjectChanged(() => {
      if (refreshTimer.current) clearTimeout(refreshTimer.current);
      refreshTimer.current = setTimeout(() => {
        void (async () => {
          try {
            await loadProjects();
            const id = selectedIdRef.current;
            if (id) await openProject(id, false, false);
          } catch (reason) {
            setError(messageFrom(reason));
          }
        })();
      }, 350);
    })
      .then((stop) => {
        unlisten = stop;
      })
      .catch((reason) => setError(`Automatic refresh is unavailable: ${messageFrom(reason)}`));
    return () => {
      unlisten?.();
      if (refreshTimer.current) clearTimeout(refreshTimer.current);
    };
  }, [loadProjects, openProject]);

  useEffect(() => {
    if (!selectedId) return;
    let active = true;
    const updatePendingReviews = async () => {
      try {
        const pendingReviews = await api.listPendingReviews(selectedId);
        if (!active) return;
        setDetail((current) => current?.project.id === selectedId
          ? {
              ...current,
              state: { ...current.state, pendingReviews },
            }
          : current);
      } catch (reason) {
        if (active) {
          setError(`Completion review updates are unavailable: ${messageFrom(reason)}`);
        }
      }
    };
    void updatePendingReviews();
    const timer = window.setInterval(() => void updatePendingReviews(), 2_000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [selectedId]);

  useEffect(() => {
    if (!selectedId || tab !== "codex") return;
    let active = true;
    const updateCodexMonitoring = async () => {
      try {
        const monitoring = await api.listCodexSessions(selectedId);
        if (active) setCodexMonitoring(monitoring);
      } catch (reason) {
        if (active) setError(`Codex monitoring updates are unavailable: ${messageFrom(reason)}`);
      }
    };
    setCodexMonitoring(null);
    void updateCodexMonitoring();
    const timer = window.setInterval(() => void updateCodexMonitoring(), 2_000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [selectedId, tab]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void api.onGitSyncChanged(() => {
      void (async () => {
        try {
          await loadProjects();
          const id = selectedIdRef.current;
          if (id) await openProject(id, false, false);
        } catch (reason) {
          setError(messageFrom(reason));
        }
      })();
    })
      .then((stop) => {
        unlisten = stop;
      })
      .catch((reason) => setError(`Git synchronization updates are unavailable: ${messageFrom(reason)}`));
    return () => unlisten?.();
  }, [loadProjects, openProject]);

  const register = async () => {
    setError(null);
    try {
      const path = await api.chooseProjectDirectory();
      if (!path) return;
      const project = await api.registerProject(path);
      await loadProjects();
      await openProject(project.id);
    } catch (reason) {
      setError(messageFrom(reason));
    }
  };

  const remove = async () => {
    if (!detail) return;
    const confirmed = window.confirm(
      `Remove ${detail.project.name} from Projector? Its files will not be changed.`,
    );
    if (!confirmed) return;

    try {
      await api.removeProject(detail.project.id);
      const nextProjects = await loadProjects();
      setDetail(null);
      setSelectedId(null);
      if (nextProjects[0]) await openProject(nextProjects[0].id);
    } catch (reason) {
      setError(messageFrom(reason));
    }
  };

  const refresh = async () => {
    try {
      setError(null);
      const nextProjects = await api.refreshProjects();
      setProjects(nextProjects);
      if (selectedIdRef.current) await openProject(selectedIdRef.current, false);
    } catch (reason) {
      setError(messageFrom(reason));
    }
  };

  const beginCreate = () => {
    setError(null);
    setNewProjectName("");
    setNewProjectParent("");
    setInitializeGit(true);
    setCreateDialogOpen(true);
  };

  const chooseCreateLocation = async () => {
    try {
      const path = await api.chooseProjectParentDirectory();
      if (path) setNewProjectParent(path);
    } catch (reason) {
      setError(messageFrom(reason));
    }
  };

  const create = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!newProjectName.trim() || !newProjectParent) return;
    setError(null);
    setCreatingProject(true);
    try {
      const project = await api.createProject(
        newProjectParent,
        newProjectName.trim(),
        initializeGit,
      );
      setCreateDialogOpen(false);
      setSettingsOpen(false);
      await loadProjects();
      await openProject(project.id);
    } catch (reason) {
      setError(messageFrom(reason));
    } finally {
      setCreatingProject(false);
    }
  };

  const pull = async () => {
    const id = selectedIdRef.current;
    if (!id) return;
    try {
      setError(null);
      setPulling(true);
      const nextDetail = await api.pullProject(id);
      setDetail(nextDetail);
      await loadProjects();
    } catch (reason) {
      setError(messageFrom(reason));
    } finally {
      setPulling(false);
    }
  };

  const approveCompletion = async (proposalId: string) => {
    const id = selectedIdRef.current;
    if (!id) return;
    try {
      setError(null);
      setReviewingProposal(proposalId);
      await api.approveCompletion(id, proposalId);
      await openProject(id, false, false);
    } catch (reason) {
      setError(messageFrom(reason));
    } finally {
      setReviewingProposal(null);
    }
  };

  const rejectCompletion = async (proposalId: string) => {
    const id = selectedIdRef.current;
    if (!id) return;
    try {
      setError(null);
      setReviewingProposal(proposalId);
      await api.rejectCompletion(id, proposalId);
      await openProject(id, false, false);
    } catch (reason) {
      setError(messageFrom(reason));
    } finally {
      setReviewingProposal(null);
    }
  };

  const linkCodexSession = async (sessionId: string) => {
    const id = selectedIdRef.current;
    if (!id) return;
    try {
      setError(null);
      setCodexBusySession(sessionId);
      setCodexMonitoring(await api.linkCodexSession(id, sessionId));
    } catch (reason) {
      setError(`Unable to link Codex session: ${messageFrom(reason)}`);
    } finally {
      setCodexBusySession(null);
    }
  };

  const unlinkCodexSession = async (sessionId: string) => {
    const id = selectedIdRef.current;
    if (!id) return;
    try {
      setError(null);
      setCodexBusySession(sessionId);
      setCodexMonitoring(await api.unlinkCodexSession(id, sessionId));
    } catch (reason) {
      setError(`Unable to unlink Codex session: ${messageFrom(reason)}`);
    } finally {
      setCodexBusySession(null);
    }
  };

  const openSettings = async () => {
    setSettingsOpen(true);
    setSettingsLoading(true);
    setSettingsMessage(null);
    setSettingsPreview(null);
    setError(null);
    try {
      setSubagentSettings(await api.getSubagentSettings());
    } catch (reason) {
      setSubagentSettings(null);
      setError(messageFrom(reason));
    } finally {
      setSettingsLoading(false);
    }
  };

  const saveSettings = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!subagentSettings) return;
    setSettingsSaving(true);
    setSettingsMessage(null);
    setError(null);
    try {
      setSubagentSettings(await api.saveSubagentSettings(subagentSettings));
      setSettingsMessage("Project and subagent settings saved.");
    } catch (reason) {
      setError(messageFrom(reason));
    } finally {
      setSettingsSaving(false);
    }
  };

  const migrateSettings = async () => {
    if (!subagentSettings || migrationProjectIds.length === 0) return;
    setSettingsMigrating(true);
    setSettingsMessage(null);
    setError(null);
    try {
      const saved = await api.saveSubagentSettings(subagentSettings);
      setSubagentSettings(saved);
      const results = await api.migrateProjectSettings(migrationProjectIds);
      const changed = results.filter((result) => result.updatedFiles.length > 0).length;
      const failures = results.filter((result) => result.error);
      setSettingsMessage(
        `Settings saved. Migrated ${results.length - failures.length} of ${results.length} selected ${results.length === 1 ? "project" : "projects"}; ${changed} had file changes.`,
      );
      if (failures.length > 0) {
        setError(failures.map((result) => `${result.projectName}: ${result.error}`).join(" "));
      }
    } catch (reason) {
      setError(messageFrom(reason));
    } finally {
      setSettingsMigrating(false);
    }
  };

  const resetSettings = async () => {
    setSettingsSaving(true);
    setSettingsMessage(null);
    setError(null);
    try {
      setSubagentSettings(await api.resetSubagentSettings());
      setSettingsPreview(null);
      setSettingsMessage("Bundled Projector and subagent defaults restored.");
    } catch (reason) {
      setError(messageFrom(reason));
    } finally {
      setSettingsSaving(false);
    }
  };

  const previewSettings = async () => {
    if (!subagentSettings) return;
    setSettingsMessage(null);
    setError(null);
    try {
      setSettingsPreview(await api.previewSubagentFiles(subagentSettings));
    } catch (reason) {
      setError(messageFrom(reason));
    }
  };

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <header className="brand">
          <div>
            <p className="eyebrow">Local project observer</p>
            <h1>Projector</h1>
          </div>
          <button className="icon-button" onClick={() => void refresh()} title="Refresh projects" aria-label="Refresh projects">
            ↻
          </button>
        </header>

        <div className="project-actions">
          <button className="primary-button register-button" onClick={beginCreate}>
            <span aria-hidden="true">＋</span> Create project
          </button>
          <button className="secondary-button register-button" onClick={() => void register()}>
            Register folder
          </button>
        </div>

        <div className="project-count">
          {projects.length} {projects.length === 1 ? "project" : "projects"}
        </div>
        <nav className="project-list" aria-label="Registered projects">
          {projects.map((project) => (
            <ProjectListItem
              key={project.id}
              project={project}
              selected={!settingsOpen && project.id === selectedId}
              onSelect={() => {
                setSettingsOpen(false);
                void openProject(project.id);
              }}
              onOpenRoot={() => {
                void api.openProjectRoot(project.id).catch((reason: unknown) => {
                  setError(reason instanceof Error ? reason.message : String(reason));
                });
              }}
            />
          ))}
        </nav>
        {!loading && projects.length === 0 && (
          <div className="sidebar-empty">
            Create a project or register an existing local folder to get started.
          </div>
        )}
        <button
          className={`settings-button${settingsOpen ? " selected" : ""}`}
          onClick={() => void openSettings()}
        >
          Settings
        </button>
      </aside>

      <main className="workspace">
        {error && (
          <div className="error-banner" role="alert">
            <span>{error}</span>
            <button onClick={() => setError(null)} aria-label="Dismiss error">×</button>
          </div>
        )}
        {settingsOpen ? (
          <SubagentSettingsPage
            loading={settingsLoading}
            message={settingsMessage}
            onChange={(settings) => {
              setSubagentSettings(settings);
              setSettingsMessage(null);
              setSettingsPreview(null);
            }}
            onPreview={() => void previewSettings()}
            onMigrate={() => void migrateSettings()}
            onMigrationProjectIds={setMigrationProjectIds}
            onReset={() => void resetSettings()}
            onSave={(event) => void saveSettings(event)}
            preview={settingsPreview}
            saving={settingsSaving}
            migrating={settingsMigrating}
            migrationProjectIds={migrationProjectIds}
            projects={projects}
            settings={subagentSettings}
          />
        ) : loading ? (
          <CenteredMessage title="Loading projects…" />
        ) : detail ? (
          <ProjectView
            detail={detail}
            tab={tab}
            refreshing={refreshing}
            pulling={pulling}
            reviewingProposal={reviewingProposal}
            onTab={setTab}
            onRefresh={() => void refresh()}
            onPull={() => void pull()}
            onRemove={() => void remove()}
            onApproveCompletion={(proposalId) => void approveCompletion(proposalId)}
            onRejectCompletion={(proposalId) => void rejectCompletion(proposalId)}
            codexMonitoring={codexMonitoring}
            codexBusySession={codexBusySession}
            onLinkCodexSession={(sessionId) => void linkCodexSession(sessionId)}
            onUnlinkCodexSession={(sessionId) => void unlinkCodexSession(sessionId)}
          />
        ) : (
          <CenteredMessage
            title={projects.length ? "Select a project" : "Your projects, at a glance"}
            body={projects.length ? "Choose a registered project from the list." : "Create a Projector-ready folder or register an existing project. Projector never runs your project."}
            action={!projects.length ? (
              <div className="empty-actions">
                <button className="primary-button" onClick={beginCreate}>Create your first project</button>
                <button className="secondary-button" onClick={() => void register()}>Register an existing folder</button>
              </div>
            ) : undefined}
          />
        )}
      </main>
      {createDialogOpen && (
        <DetailWindow
          id="create-project"
          initialFocusRef={createNameInput}
          onClose={() => {
            if (!creatingProject) setCreateDialogOpen(false);
          }}
          title="Create a project"
        >
          <form className="create-project-form" onSubmit={(event) => void create(event)}>
            <label>
              Project name
              <input
                maxLength={120}
                onChange={(event) => setNewProjectName(event.target.value)}
                placeholder="My Project"
                ref={createNameInput}
                required
                value={newProjectName}
              />
            </label>
            <label>
              Parent folder
              <div className="path-picker">
                <input
                  aria-label="Parent folder"
                  placeholder="Choose a folder"
                  readOnly
                  required
                  value={newProjectParent}
                />
                <button
                  className="secondary-button"
                  onClick={() => void chooseCreateLocation()}
                  type="button"
                >
                  Choose…
                </button>
              </div>
            </label>
            <label className="create-project-option">
              <input
                checked={initializeGit}
                onChange={(event) => setInitializeGit(event.target.checked)}
                type="checkbox"
              />
              Initialize a Git repository
            </label>
            <p className="create-project-note">
              Projector creates README.md, STARTUP.md, AGENTS.md, TODO.md, WORK_HISTORY.md, and the configured .codex/agents workers, then registers the folder. It does not create a commit or remote.
            </p>
            <button
              aria-label="Confirm project creation"
              className="primary-button"
              disabled={creatingProject || !newProjectName.trim() || !newProjectParent}
              type="submit"
            >
              {creatingProject ? "Creating…" : "Create project"}
            </button>
          </form>
        </DetailWindow>
      )}
    </div>
  );
}

function ProjectListItem({
  project,
  selected,
  onSelect,
  onOpenRoot,
}: {
  project: ProjectSummary;
  selected: boolean;
  onSelect: () => void;
  onOpenRoot: () => void;
}) {
  return (
    <div className={`project-list-item${selected ? " selected" : ""}`}>
      <button className="project-list-select" onClick={onSelect}>
      <span className="project-list-name">{project.name}</span>
      <span className="project-list-path" title={project.path}>{project.path}</span>
      <span className="project-list-meta">
        <GitBadge git={project.git} />
        <span>{project.git.branch ?? "No Git"}</span>
        {project.git.isRepository && <span>{syncStatusLabel(project.git)}</span>}
        <span className="dot">·</span>
        <span>{formatRelative(project.git.lastActivity ?? project.lastOpened)}</span>
      </span>
      </button>
      <button
        aria-label={`Open ${project.name} folder`}
        className="project-folder-button"
        onClick={onOpenRoot}
        title="Open project folder"
        type="button"
      >
        <svg aria-hidden="true" viewBox="0 0 24 24">
          <path d="M3.5 6.75A2.25 2.25 0 0 1 5.75 4.5h4.06l1.7 2H18.25A2.25 2.25 0 0 1 20.5 8.75v8.5a2.25 2.25 0 0 1-2.25 2.25H5.75a2.25 2.25 0 0 1-2.25-2.25v-10.5Z" />
        </svg>
      </button>
    </div>
  );
}

function ProjectView({
  detail,
  tab,
  refreshing,
  pulling,
  reviewingProposal,
  onTab,
  onRefresh,
  onPull,
  onRemove,
  onApproveCompletion,
  onRejectCompletion,
  codexMonitoring,
  codexBusySession,
  onLinkCodexSession,
  onUnlinkCodexSession,
}: {
  detail: ProjectDetail;
  tab: Tab;
  refreshing: boolean;
  pulling: boolean;
  reviewingProposal: string | null;
  onTab: (tab: Tab) => void;
  onRefresh: () => void;
  onPull: () => void;
  onRemove: () => void;
  onApproveCompletion: (proposalId: string) => void;
  onRejectCompletion: (proposalId: string) => void;
  codexMonitoring: CodexMonitoringSnapshot | null;
  codexBusySession: string | null;
  onLinkCodexSession: (sessionId: string) => void;
  onUnlinkCodexSession: (sessionId: string) => void;
}) {
  const pullUnavailableReason = getPullUnavailableReason(detail.project.git);
  return (
    <div className="project-view">
      <header className="project-header">
        <div className="project-title-block">
          <div className="title-row">
            <h2>{detail.project.name}</h2>
            <GitBadge git={detail.project.git} verbose />
          </div>
          <p title={detail.project.path}>{detail.project.path}</p>
        </div>
        <div className="header-actions">
          <button
            className="secondary-button pull-button"
            onClick={onPull}
            disabled={pulling || pullUnavailableReason !== null}
            title={pullUnavailableReason ?? "Fast-forward the current branch from its upstream"}
          >
            {pulling ? "Pulling…" : "Pull"}
          </button>
          <button className="secondary-button" onClick={onRefresh} disabled={refreshing}>
            {refreshing ? "Refreshing…" : "Refresh"}
          </button>
          <button className="text-button danger" onClick={onRemove}>Remove</button>
        </div>
      </header>

      <div className="tabs" role="tablist" aria-label="Project information">
        {tabs.map((item) => (
          <button
            key={item.id}
            role="tab"
            aria-selected={tab === item.id}
            className={tab === item.id ? "active" : ""}
            onClick={() => onTab(item.id)}
          >
            {item.label}
            {item.id === "pending" && detail.state.pendingReviews.length > 0 && (
              <span className="tab-count">{detail.state.pendingReviews.length}</span>
            )}
          </button>
        ))}
      </div>

      <section className="tab-content">
        {tab === "overview" && <Overview detail={detail} />}
        {tab === "readme" && <DocumentPanel document={detail.documents.readme} />}
        {tab === "startup" && <DocumentPanel copyPowerShell document={detail.documents.startup} />}
        {tab === "todo" && (
          <TodoPanel document={detail.state.todos} source={detail.documents.todo} />
        )}
        {tab === "pending" && (
          <PendingReviewPanel
            proposals={detail.state.pendingReviews}
            reviewingProposal={reviewingProposal}
            onApprove={onApproveCompletion}
            onReject={onRejectCompletion}
          />
        )}
        {tab === "history" && (
          <HistoryPanel
            document={detail.state.workingHistory}
            source={detail.documents.workingHistory}
          />
        )}
        {tab === "git" && <GitPanel git={detail.project.git} />}
        {tab === "codex" && (
          <CodexPanel
            busySession={codexBusySession}
            monitoring={codexMonitoring}
            onLink={onLinkCodexSession}
            onUnlink={onUnlinkCodexSession}
          />
        )}
      </section>
    </div>
  );
}

function Overview({ detail }: { detail: ProjectDetail }) {
  const documents = [detail.documents.readme, detail.documents.todo, detail.documents.workingHistory];
  return (
    <div className="overview-grid">
      <section className="panel git-overview">
        <p className="section-label">Repository</p>
        {detail.project.git.isRepository ? (
          <>
            <div className="fact-grid">
              <Fact label="Branch" value={detail.project.git.branch ?? "Unknown"} />
              <Fact label="Working tree" value={detail.project.git.dirty === null ? "Unavailable" : detail.project.git.dirty ? "Uncommitted changes" : "Clean"} />
              <Fact label="Upstream status" value={syncStatusLabel(detail.project.git)} />
              <Fact label="Upstream" value={detail.project.git.upstream ?? "Not configured"} />
              <Fact label="Last successful fetch" value={formatDate(detail.project.git.lastSuccessfulFetch)} />
              <Fact label="Last activity" value={formatDate(detail.project.git.lastActivity)} />
              <Fact label="Last opened" value={formatDate(detail.project.lastOpened)} />
            </div>
            {detail.project.git.fetchStatus === "fetching" && <div className="notice">Fetching all remotes in the background…</div>}
            {detail.project.git.fetchError && <div className={`notice ${detail.project.git.fetchStatus === "noRemote" ? "" : "error"}`}>{detail.project.git.fetchError}</div>}
          </>
        ) : (
          <StatusMessage title="Not a Git repository" body={detail.project.git.error ?? "Documentation remains available."} />
        )}
      </section>

      <section className="panel document-overview">
        <p className="section-label">Project documents</p>
        <div className="document-status-list">
          {documents.map((document) => (
            <div className="document-status" key={document.name}>
              <span className={`status-dot ${document.status}`} />
              <div>
                <strong>{document.name}</strong>
                <p>{document.relativePath ?? (document.status === "missing" ? "Not found in project root or docs/" : document.error)}</p>
              </div>
              {document.modifiedAt && <time>{formatRelative(document.modifiedAt)}</time>}
            </div>
          ))}
        </div>
      </section>

      <section className="panel recent-overview">
        <p className="section-label">Recent Git activity</p>
        {detail.project.git.recentCommits.length ? (
          <CommitList commits={detail.project.git.recentCommits.slice(0, 5)} />
        ) : (
          <StatusMessage title="No commits to show" body="Recent commits will appear here when available." />
        )}
      </section>
    </div>
  );
}

export function DocumentPanel({
  copyPowerShell = false,
  document,
}: {
  copyPowerShell?: boolean;
  document: ProjectDocument;
}) {
  if (document.status === "missing") {
    return <StatusMessage title={`${document.name} was not found`} body={`Add ${document.name} to the project root or docs/ directory to make it visible here.`} />;
  }
  if (document.status === "error") {
    return <StatusMessage title={`${document.name} could not be read`} body={document.error ?? "The file is inaccessible."} tone="error" />;
  }
  return (
    <article className="document-panel">
      <div className="document-meta">
        <span>{document.relativePath}</span>
        {document.modifiedAt && <span>Updated {formatDate(document.modifiedAt)}</span>}
      </div>
      {document.truncated && (
        <div className="notice">This file is larger than 2 MB. Projector is showing the first 2 MB to stay responsive.</div>
      )}
      <div className="markdown-body">
        <Markdown
          components={copyPowerShell ? {
            a: ExternalBrowserLink,
            pre: CopyablePowerShellPre,
          } : undefined}
          remarkPlugins={[remarkGfm]}
          rehypePlugins={[rehypeRaw, [rehypeSanitize, markdownSanitizeSchema]]}
        >
          {document.content ?? ""}
        </Markdown>
      </div>
    </article>
  );
}

function ExternalBrowserLink({ children, href }: { children?: ReactNode; href?: string }) {
  const [failed, setFailed] = useState(false);
  const isExternal = href?.startsWith("https://") || href?.startsWith("http://");
  if (!isExternal || !href) return <a href={href}>{children}</a>;

  const open = async (event: MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault();
    try {
      await api.openExternalUrl(href);
      setFailed(false);
    } catch {
      setFailed(true);
    }
  };

  return (
    <>
      <a href={href} onClick={open}>{children}</a>
      {failed && <span className="link-open-error" role="alert"> Could not open link.</span>}
    </>
  );
}

function CopyablePowerShellPre({ children }: { children?: ReactNode }) {
  const [copyState, setCopyState] = useState<"idle" | "copied" | "error">("idle");
  if (!isValidElement<{ className?: string; children?: ReactNode }>(children)
    || !children.props.className?.split(" ").includes("language-powershell")) {
    return <pre>{children}</pre>;
  }

  const script = reactNodeText(children.props.children).replace(/\n$/, "");
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(script);
      setCopyState("copied");
    } catch {
      setCopyState("error");
    }
  };

  return (
    <div className="powershell-block">
      <button className="code-copy-button" onClick={copy} type="button">
        {copyState === "copied" ? "Copied" : copyState === "error" ? "Copy failed" : "Copy"}
      </button>
      <pre>{children}</pre>
    </div>
  );
}

function reactNodeText(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(reactNodeText).join("");
  return isValidElement<{ children?: ReactNode }>(node) ? reactNodeText(node.props.children) : "";
}

function GitPanel({ git }: { git: GitInfo }) {
  if (!git.isRepository) {
    return <StatusMessage title="Not a Git repository" body={git.error ?? "This project can still be observed through its Markdown files."} />;
  }
  return (
    <div className="git-panel">
      {git.error && <div className="notice error">{git.error}</div>}
      {git.fetchStatus === "fetching" && <div className="notice">Fetching all remotes in the background…</div>}
      {git.fetchError && <div className={`notice ${git.fetchStatus === "noRemote" ? "" : "error"}`}>{git.fetchError}</div>}
      <div className="fact-grid panel compact">
        <Fact label="Branch" value={git.branch ?? "Unknown"} />
        <Fact label="Working tree" value={git.dirty === null ? "Unavailable" : git.dirty ? "Uncommitted changes" : "Clean"} />
        <Fact label="Upstream status" value={syncStatusLabel(git)} />
        <Fact label="Upstream branch" value={git.upstream ?? "Not configured"} />
        <Fact label="Ahead / behind" value={git.ahead === null || git.behind === null ? "Unknown" : `${git.ahead} / ${git.behind}`} />
        <Fact label="Last successful fetch" value={formatDate(git.lastSuccessfulFetch)} />
        <Fact label="Last activity" value={formatDate(git.lastActivity)} />
        <Fact label="Commits shown" value={String(git.recentCommits.length)} />
      </div>
      {git.syncMessage && <div className="notice">{git.syncMessage}</div>}
      <section className="panel commits-panel">
        <p className="section-label">Recent commits</p>
        {git.recentCommits.length ? <CommitList commits={git.recentCommits} /> : <StatusMessage title="No commits to show" />}
      </section>
    </div>
  );
}

export function CodexPanel({
  monitoring,
  busySession,
  onLink,
  onUnlink,
}: {
  monitoring: CodexMonitoringSnapshot | null;
  busySession: string | null;
  onLink: (sessionId: string) => void;
  onUnlink: (sessionId: string) => void;
}) {
  const [selectedSession, setSelectedSession] = useState("");
  useEffect(() => {
    if (!monitoring?.detectedSessions.some((session) => session.sessionId === selectedSession)) {
      setSelectedSession(monitoring?.detectedSessions[0]?.sessionId ?? "");
    }
  }, [monitoring, selectedSession]);

  if (!monitoring) {
    return <StatusMessage title="Loading Codex sessionsâ€¦" />;
  }

  return (
    <div className="codex-panel">
      <section className="panel codex-link-panel">
        <div>
          <p className="section-label">Detected Codex sessions</p>
          <p>Link a locally detected session manually. Projector never reads prompts, responses, transcripts, or tool calls.</p>
        </div>
        {monitoring.detectedSessions.length > 0 ? (
          <div className="codex-link-controls">
            <label>
              Session
              <select
                aria-label="Detected Codex session"
                onChange={(event) => setSelectedSession(event.target.value)}
                value={selectedSession}
              >
                {monitoring.detectedSessions.map((session) => (
                  <option key={session.sessionId} value={session.sessionId}>
                    {session.sessionId} Â· {session.cwd}
                  </option>
                ))}
              </select>
            </label>
            <button
              className="primary-button"
              disabled={!selectedSession || busySession !== null}
              onClick={() => onLink(selectedSession)}
            >
              {busySession === selectedSession ? "Linkingâ€¦" : "Link session"}
            </button>
          </div>
        ) : (
          <div className="notice">No unlinked sessions detected. Configure the four lifecycle hooks and start or resume a Codex session.</div>
        )}
      </section>

      <section className="codex-session-list" aria-label="Linked Codex sessions">
        {monitoring.linkedSessions.length === 0 ? (
          <StatusMessage title="No Codex session linked" body="Detected sessions stay unassigned until you link one to this project." />
        ) : monitoring.linkedSessions.map((session) => (
          <article className="panel codex-session-card" key={session.sessionId}>
            <header>
              <div>
                <div className="codex-session-title">
                  <code>{session.sessionId}</code>
                  <LifecycleBadge state={session.state} />
                </div>
                <p title={session.cwd}>{session.cwd}</p>
              </div>
              <button
                className="text-button danger"
                disabled={busySession !== null}
                onClick={() => onUnlink(session.sessionId)}
              >
                {busySession === session.sessionId ? "Unlinkingâ€¦" : "Unlink"}
              </button>
            </header>
            <p className="codex-observed">Last observed {formatDate(session.lastSeenAt)}</p>

            <section>
              <p className="section-label">Subagents</p>
              {session.agents.length === 0 ? (
                <p className="codex-empty">No subagents observed in this session.</p>
              ) : (
                <div className="codex-agent-list">
                  {session.agents.map((agent) => (
                    <div className="codex-agent-row" key={agent.agentId}>
                      <div>
                        <strong>{agent.agentType}</strong>
                        <code>{agent.agentId}</code>
                      </div>
                      <LifecycleBadge state={agent.state} />
                      <time>{formatDate(agent.lastSeenAt)}</time>
                    </div>
                  ))}
                </div>
              )}
            </section>

            <section>
              <p className="section-label">Recent transitions</p>
              <ol className="codex-transition-list">
                {[...session.transitions].reverse().map((transition, index) => (
                  <li key={`${transition.observedAt}-${transition.kind}-${transition.agentId ?? "session"}-${index}`}>
                    <span>{transitionLabel(transition.kind, transition.agentType)}</span>
                    <time>{formatDate(transition.observedAt)}</time>
                  </li>
                ))}
              </ol>
            </section>
          </article>
        ))}
      </section>
    </div>
  );
}

function LifecycleBadge({ state }: { state: CodexLifecycleState }) {
  return <span className={`lifecycle-badge ${state}`}>{state}</span>;
}

function transitionLabel(kind: CodexTransitionKind, agentType: string | null): string {
  const subject = agentType ? `${agentType} subagent` : "Session";
  switch (kind) {
    case "sessionStarted": return "Session started";
    case "sessionStopped": return "Session stopped";
    case "sessionUnknown": return "Session state became unknown after restart";
    case "subagentStarted": return `${subject} started`;
    case "subagentStopped": return `${subject} stopped`;
    case "subagentUnknown": return `${subject} state became unknown`;
  }
}

function CommitList({ commits }: { commits: GitInfo["recentCommits"] }) {
  return (
    <ol className="commit-list">
      {commits.map((commit) => (
        <li key={commit.id}>
          <code>{commit.id}</code>
          <div>
            <strong>{commit.summary}</strong>
            <p>{commit.author} · {formatDate(commit.committedAt)}</p>
          </div>
        </li>
      ))}
    </ol>
  );
}

function Fact({ label, value }: { label: string; value: string }) {
  return <div className="fact"><span>{label}</span><strong>{value}</strong></div>;
}

function GitBadge({ git, verbose = false }: { git: GitInfo; verbose?: boolean }) {
  const label = !git.isRepository ? "No Git" : git.dirty === null ? "Git unavailable" : git.dirty ? (verbose ? "Uncommitted changes" : "Dirty") : "Clean";
  const state = !git.isRepository || git.dirty === null ? "neutral" : git.dirty ? "dirty" : "clean";
  return <span className={`git-badge ${state}`}><span />{label}</span>;
}

function syncStatusLabel(git: GitInfo): string {
  switch (git.syncStatus) {
    case "ahead":
      return `Ahead by ${git.ahead ?? "?"}`;
    case "behind":
      return `Behind by ${git.behind ?? "?"}`;
    case "diverged":
      return `Diverged (${git.ahead ?? "?"} ahead, ${git.behind ?? "?"} behind)`;
    case "synchronized":
      return "Synchronized";
    default:
      return "Upstream unknown";
  }
}

function SubagentSettingsPage({
  loading,
  message,
  onChange,
  onMigrate,
  onMigrationProjectIds,
  onPreview,
  onReset,
  onSave,
  preview,
  projects,
  saving,
  migrating,
  migrationProjectIds,
  settings,
}: {
  loading: boolean;
  message: string | null;
  onChange: (settings: SubagentSettings) => void;
  onMigrate: () => void;
  onMigrationProjectIds: (ids: string[]) => void;
  onPreview: () => void;
  onReset: () => void;
  onSave: (event: FormEvent<HTMLFormElement>) => void;
  preview: GeneratedSubagentFile[] | null;
  projects: ProjectSummary[];
  saving: boolean;
  migrating: boolean;
  migrationProjectIds: string[];
  settings: SubagentSettings | null;
}) {
  if (loading) return <CenteredMessage title="Loading settings…" />;
  if (!settings) {
    return <CenteredMessage title="Project settings are unavailable" body="Review the error above and try again." />;
  }

  const updateWorker = <K extends keyof SubagentSettings[WorkerKey]>(
    workerKey: WorkerKey,
    field: K,
    value: SubagentSettings[WorkerKey][K],
  ) => {
    onChange({
      ...settings,
      [workerKey]: { ...settings[workerKey], [field]: value },
    });
  };

  return (
    <div className="settings-page">
      <header className="settings-header">
        <div>
          <p className="eyebrow">Project configuration</p>
          <h2>Project settings</h2>
          <p>New projects use these defaults. You can also explicitly migrate selected registered projects below.</p>
        </div>
      </header>
      <form className="settings-form" onSubmit={onSave}>
        {message && <div className="notice success" role="status">{message}</div>}
        <section className="panel settings-section">
          <h3>Projector</h3>
          <label htmlFor="projector-section">AGENTS.md Projector section</label>
          <p>Edit the complete Markdown section. It must begin with <code>## Projector</code>.</p>
          <textarea
            id="projector-section"
            maxLength={32_000}
            onChange={(event) => onChange({ ...settings, projectorSection: event.target.value })}
            required
            rows={18}
            value={settings.projectorSection}
          />
        </section>

        <section className="panel settings-section">
          <h3>Subagents</h3>
          <label htmlFor="subagents-section">AGENTS.md Subagents section</label>
            <p>Edit the complete Markdown section. It must begin with <code>## Subagents</code>.</p>
          <textarea
            id="subagents-section"
            maxLength={32_000}
            onChange={(event) => onChange({ ...settings, subagentsSection: event.target.value })}
            required
            rows={18}
            value={settings.subagentsSection}
          />
        </section>

        <div className="worker-settings-grid">
          {workerDefinitions.map(({ key, label }) => {
            const worker = settings[key];
            return (
              <section className="panel worker-settings-card" key={key}>
                <header>
                  <div>
                    <p className="section-label">{worker.name}</p>
                    <h3>{label}</h3>
                  </div>
                  <code>.codex/agents/{worker.fileName}</code>
                </header>
                <label>
                  Model
                  <input
                    aria-label={`${label} model`}
                    maxLength={200}
                    onChange={(event) => updateWorker(key, "model", event.target.value)}
                    required
                    value={worker.model}
                  />
                </label>
                <label>
                  Reasoning effort
                  <select
                    aria-label={`${label} reasoning effort`}
                    onChange={(event) => updateWorker(key, "modelReasoningEffort", event.target.value as ReasoningEffort)}
                    value={worker.modelReasoningEffort}
                  >
                    {reasoningEfforts.map((effort) => <option key={effort} value={effort}>{effort}</option>)}
                  </select>
                </label>
                <label>
                  Description
                  <textarea
                    aria-label={`${label} description`}
                    maxLength={2_000}
                    onChange={(event) => updateWorker(key, "description", event.target.value)}
                    required
                    rows={3}
                    value={worker.description}
                  />
                </label>
                <label>
                  Developer instructions
                  <textarea
                    aria-label={`${label} developer instructions`}
                    maxLength={32_000}
                    onChange={(event) => updateWorker(key, "developerInstructions", event.target.value)}
                    required
                    rows={14}
                    value={worker.developerInstructions}
                  />
                </label>
                <p className="fixed-setting">Sandbox: <code>{worker.sandboxMode}</code></p>
              </section>
            );
          })}
        </div>

        <div className="settings-actions">
          <button className="primary-button" disabled={saving} type="submit">
            {saving ? "Saving…" : "Save settings"}
          </button>
          <button className="secondary-button" disabled={saving} onClick={onPreview} type="button">
            Preview generated files
          </button>
            <button className="text-button" disabled={saving || migrating} onClick={onReset} type="button">
            Reset to bundled defaults
          </button>
        </div>

        <section className="panel settings-section migration-settings" aria-label="Project migration">
          <h3>Migrate registered projects</h3>
          <p>Apply the current Projector section, Subagents section, and three worker TOMLs. Other AGENTS.md sections are preserved. Originals receive a <code>.projector-backup</code> before their first change.</p>
          {projects.length === 0 ? (
            <p>No registered projects are available.</p>
          ) : (
            <>
              <label className="migration-project-option">
                <input
                  checked={migrationProjectIds.length === projects.length}
                  onChange={(event) => onMigrationProjectIds(event.target.checked ? projects.map((project) => project.id) : [])}
                  type="checkbox"
                />
                Select all registered projects
              </label>
              <div className="migration-project-list">
                {projects.map((project) => (
                  <label className="migration-project-option" key={project.id}>
                    <input
                      checked={migrationProjectIds.includes(project.id)}
                      onChange={(event) => onMigrationProjectIds(
                        event.target.checked
                          ? [...migrationProjectIds, project.id]
                          : migrationProjectIds.filter((id) => id !== project.id),
                      )}
                      type="checkbox"
                    />
                    <span><strong>{project.name}</strong><small>{project.path}</small></span>
                  </label>
                ))}
              </div>
              <button
                className="secondary-button"
                disabled={migrating || saving || migrationProjectIds.length === 0}
                onClick={onMigrate}
                type="button"
              >
                {migrating ? "Migrating…" : "Save and migrate selected"}
              </button>
            </>
          )}
        </section>

        {preview && (
          <section className="panel generated-preview" aria-label="Generated file preview">
            <p className="section-label">Generated file preview</p>
            {preview.map((file) => (
              <details key={file.path}>
                <summary>{file.path}</summary>
                <pre>{file.content}</pre>
              </details>
            ))}
          </section>
        )}
      </form>
    </div>
  );
}

export function TodoPanel({
  document,
  source,
}: {
  document: TodoDocument;
  source: ProjectDocument;
}) {
  const [selectedTodoId, setSelectedTodoId] = useState<string | null>(null);
  const [category, setCategory] = useState<WorkCategory | "all">("all");

  if (source.status === "missing") {
    return (
      <StatusMessage
        title="TODO.md was not found"
        body="Projector's add_todo operation can create the first structured TODO."
      />
    );
  }
  if (source.status === "error") {
    return (
      <StatusMessage
        title="TODO.md could not be read"
        body={source.error ?? "The file is inaccessible."}
        tone="error"
      />
    );
  }

  const priorities: TodoPriority[] = ["critical", "high", "medium", "low"];
  const categories = workCategories.filter((value) =>
    document.items.some((item) => item.category === value),
  );
  const filteredItems = document.items.filter(
    (item) => category === "all" || item.category === category,
  );
  const selectedTodo = document.items.find((item) => item.id === selectedTodoId) ?? null;

  return (
    <div className="structured-panel">
      <StructuredHeader
        path={document.relativePath}
        count={document.items.length}
        noun="TODO"
      />
      <ValidationWarnings warnings={document.warnings} />
      {document.items.length > 0 && (
        <div className="history-controls todo-controls panel" aria-label="TODO filters">
          <label>
            Category
            <select
              value={category}
              onChange={(event) => {
                setCategory(event.target.value as WorkCategory | "all");
                setSelectedTodoId(null);
              }}
            >
              <option value="all">All categories</option>
              {categories.map((value) => (
                <option key={value} value={value}>{value}</option>
              ))}
            </select>
          </label>
        </div>
      )}
      {document.items.length ? (
        <div className="priority-board" aria-label="TODOs by priority">
          {priorities.map((priority) => {
            const items = filteredItems.filter((item) => item.priority === priority);
            return (
              <section className={`priority-column priority-${priority}`} key={priority}>
                <div className="priority-column-heading">
                  <h3>{priority}</h3>
                  <span>{items.length}</span>
                </div>
                <div className="priority-column-items">
                  {items.length ? items.map((item) => {
                    const selected = selectedTodoId === item.id;
                    const status = todoStatus(item.dependencies);
                    return (
                      <article
                        className={`todo-card${status === "blocked" ? " blocked" : ""}${selected ? " selected" : ""}`}
                        key={item.id}
                      >
                        <button
                          aria-haspopup="dialog"
                          aria-label={`Open ${item.title}`}
                          className="todo-summary"
                          id={`todo-${item.id}`}
                          onClick={() => setSelectedTodoId(item.id)}
                          type="button"
                        >
                          <span className="todo-title">{item.title}</span>
                          <span aria-hidden="true" className="card-open-indicator">›</span>
                          <span className="todo-metadata">
                            <code>{item.id}</code>
                            <span>{status}</span>
                            <span>{item.category}</span>
                            <span>{item.area}</span>
                          </span>
                        </button>
                      </article>
                    );
                  }) : (
                    <p className="priority-empty">No {priority} TODOs</p>
                  )}
                </div>
              </section>
            );
          })}
        </div>
      ) : (
        <StatusMessage title="No open TODOs" body="This project has no structured unfinished work." />
      )}
      <PreservedContent content={document.preservedContent} />
      {selectedTodo && (
        <DetailWindow
          id="todo-detail"
          onClose={() => setSelectedTodoId(null)}
          title={selectedTodo.title}
        >
          <div className="detail-window-metadata">
            <code>{selectedTodo.id}</code>
            <span className={`tag priority ${selectedTodo.priority}`}>{selectedTodo.priority}</span>
            <span className={`tag status ${todoStatus(selectedTodo.dependencies)}`}>
              {todoStatus(selectedTodo.dependencies)}
            </span>
            <span className="tag category">{selectedTodo.category}</span>
            <span className="tag">{selectedTodo.area}</span>
          </div>
          <section>
            <p className="section-label">Dependencies</p>
            <div className="todo-dependencies" aria-label="TODO dependencies">
              {selectedTodo.dependencies.length ? (
                selectedTodo.dependencies.map((dependency) => (
                  <code key={dependency}>{dependency}</code>
                ))
              ) : (
                <span>None</span>
              )}
            </div>
          </section>
          <section>
            <p className="section-label">Rationale</p>
            <MarkdownContent content={selectedTodo.rationale} />
          </section>
          <section>
            <p className="section-label">Acceptance criteria</p>
            <MarkdownContent content={selectedTodo.acceptanceCriteria} />
          </section>
        </DetailWindow>
      )}
    </div>
  );
}

export function PendingReviewPanel({
  proposals,
  reviewingProposal,
  onApprove,
  onReject,
}: {
  proposals: CompletionProposal[];
  reviewingProposal: string | null;
  onApprove: (proposalId: string) => void;
  onReject: (proposalId: string) => void;
}) {
  return (
    <div className="structured-panel pending-review-panel">
      <StructuredHeader
        path="Projector internal storage"
        count={proposals.length}
        noun="pending proposal"
      />
      <div className="notice">
        Review requests do not change project files until you approve them.
      </div>
      {proposals.length ? (
        <section aria-label="Pending review proposals" className="pending-review-list">
          {proposals.map((proposal) => {
            const busy = reviewingProposal === proposal.id;
            const reviewInProgress = reviewingProposal !== null;
            return (
              <article className="panel pending-review-card" key={proposal.id}>
                <header>
                  <div>
                    <h3>{proposal.proposedEntry.title}</h3>
                    <div className="detail-window-metadata">
                      <span className="tag">
                        {proposal.kind === "todoCompletion"
                          ? "TODO completion"
                          : "Working history"}
                      </span>
                      {proposal.todo && (
                        <>
                          <code>{proposal.todo.id}</code>
                          <span className={`tag priority ${proposal.todo.priority}`}>
                            {proposal.todo.priority}
                          </span>
                        </>
                      )}
                      <span className="tag category">
                        {proposal.proposedEntry.category}
                      </span>
                      <span className="tag">{proposal.proposedEntry.area}</span>
                    </div>
                  </div>
                  <time dateTime={proposal.requestedAt}>
                    Requested {formatDate(proposal.requestedAt)}
                  </time>
                </header>
                <section>
                  <p className="section-label">Proposed summary</p>
                  <MarkdownContent content={proposal.proposedEntry.summary} />
                </section>
                <section>
                  <p className="section-label">Proposed limitations</p>
                  <MarkdownContent content={proposal.proposedEntry.limitations} />
                </section>
                <footer className="review-actions">
                  <button
                    className="secondary-button danger-button"
                    disabled={reviewInProgress}
                    onClick={() => onReject(proposal.id)}
                    type="button"
                  >
                    {busy ? "Reviewing…" : "Reject"}
                  </button>
                  <button
                    className="primary-button"
                    disabled={reviewInProgress}
                    onClick={() => onApprove(proposal.id)}
                    type="button"
                  >
                    {busy ? "Reviewing…" : "Approve"}
                  </button>
                </footer>
              </article>
            );
          })}
        </section>
      ) : (
        <StatusMessage
          title="No pending reviews"
          body="Agent completion and working-history requests will appear here for approval or rejection."
        />
      )}
    </div>
  );
}

export function HistoryPanel({
  document,
  source,
}: {
  document: WorkHistoryDocument;
  source: ProjectDocument;
}) {
  const [category, setCategory] = useState("all");
  const [area, setArea] = useState("all");
  const [selectedEntryKey, setSelectedEntryKey] = useState<string | null>(null);

  if (source.status === "missing") {
    return (
      <StatusMessage
        title="WORK_HISTORY.md was not found"
        body="Projector's add_work_history operation can propose the first structured entry for review."
      />
    );
  }
  if (source.status === "error") {
    return (
      <StatusMessage
        title="WORK_HISTORY.md could not be read"
        body={source.error ?? "The file is inaccessible."}
        tone="error"
      />
    );
  }

  const filtered = document.entries
    .filter(
      (entry) =>
        (category === "all" || entry.category === category) &&
        (area === "all" || entry.area === area),
    )
    .sort(compareHistoryNewestFirst);
  const selectedEntry = document.entries.find(
    (entry) => `${entry.occurredAt}-${entry.title}` === selectedEntryKey,
  ) ?? null;

  return (
    <div className="structured-panel">
      <StructuredHeader
        path={document.relativePath}
        count={document.entries.length}
        noun="history entry"
      />
      <ValidationWarnings warnings={document.warnings} />
      <div className="history-controls panel" aria-label="Working history filters">
        <label>
          Category
          <select value={category} onChange={(event) => setCategory(event.target.value)}>
            <option value="all">All categories</option>
            {document.categories.map((value) => (
              <option key={value} value={value}>{value}</option>
            ))}
          </select>
        </label>
        <label>
          Area
          <select value={area} onChange={(event) => setArea(event.target.value)}>
            <option value="all">All areas</option>
            {document.areas.map((value) => (
              <option key={value} value={value}>{value}</option>
            ))}
          </select>
        </label>
      </div>
      {filtered.length ? (
        <div className="history-list">
          {filtered.map((entry) => {
            const entryKey = `${entry.occurredAt}-${entry.title}`;
            const selected = selectedEntryKey === entryKey;
            return (
              <article
                className={`history-card${selected ? " selected" : ""}`}
                key={entryKey}
              >
                <button
                  aria-haspopup="dialog"
                  aria-label={`Open ${entry.title}`}
                  className="history-summary"
                  onClick={() => setSelectedEntryKey(entryKey)}
                  type="button"
                >
                  <span className="history-title">{entry.title}</span>
                  <span aria-hidden="true" className="card-open-indicator">›</span>
                  <span className="history-row-metadata">
                    <time>{formatLocalDateTime(entry.occurredAt)}</time>
                    <span className="tag category">{entry.category}</span>
                    <span className="tag">{entry.area}</span>
                  </span>
                </button>
              </article>
            );
          })}
        </div>
      ) : (
        <StatusMessage title="No matching history" body="Adjust the filters to show entries." />
      )}
      <PreservedContent content={document.preservedContent} />
      {selectedEntry && (
        <DetailWindow
          id="history-detail"
          onClose={() => setSelectedEntryKey(null)}
          title={selectedEntry.title}
        >
          <div className="detail-window-metadata">
            <time>{formatLocalDateTime(selectedEntry.occurredAt)}</time>
            <span className="tag category">{selectedEntry.category}</span>
            <span className="tag">{selectedEntry.area}</span>
          </div>
          <section>
            <p className="section-label">Summary</p>
            <MarkdownContent content={selectedEntry.summary} />
          </section>
          <section>
            <p className="section-label">Limitations</p>
            <MarkdownContent content={selectedEntry.limitations} />
          </section>
        </DetailWindow>
      )}
    </div>
  );
}

function DetailWindow({
  children,
  id,
  initialFocusRef,
  onClose,
  title,
}: {
  children: ReactNode;
  id: string;
  initialFocusRef?: { current: HTMLElement | null };
  onClose: () => void;
  title: string;
}) {
  const closeButton = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    (initialFocusRef?.current ?? closeButton.current)?.focus();
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [initialFocusRef, onClose]);

  return (
    <div
      className="detail-window-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section
        aria-labelledby={`${id}-title`}
        aria-modal="true"
        className="detail-window"
        role="dialog"
      >
        <header className="detail-window-header">
          <div>
            <p className="eyebrow">Details</p>
            <h3 id={`${id}-title`}>{title}</h3>
          </div>
          <button
            aria-label="Close details"
            className="detail-window-close"
            onClick={onClose}
            ref={closeButton}
            type="button"
          >
            ×
          </button>
        </header>
        <div className="detail-window-body">{children}</div>
      </section>
    </div>
  );
}

function compareHistoryNewestFirst(left: WorkHistoryEntry, right: WorkHistoryEntry) {
  const leftTime = Date.parse(left.occurredAt);
  const rightTime = Date.parse(right.occurredAt);
  if (!Number.isNaN(leftTime) && !Number.isNaN(rightTime)) {
    return rightTime - leftTime;
  }
  return right.occurredAt.localeCompare(left.occurredAt);
}

function StructuredHeader({
  path,
  count,
  noun,
}: {
  path: string | null;
  count: number;
  noun: string;
}) {
  return (
    <div className="structured-header">
      <code>{path ?? "Project document"}</code>
      <span>{count} {count === 1 ? noun : `${noun}s`}</span>
    </div>
  );
}

function ValidationWarnings({ warnings }: { warnings: ValidationWarning[] }) {
  if (!warnings.length) return null;
  return (
    <section className="validation-warnings" role="alert">
      <strong>{warnings.length} validation {warnings.length === 1 ? "warning" : "warnings"}</strong>
      <ul>
        {warnings.map((warning, index) => (
          <li key={`${warning.code}-${warning.itemId ?? "document"}-${index}`}>
            {warning.itemId && <code>{warning.itemId}</code>} {warning.message}
          </li>
        ))}
      </ul>
    </section>
  );
}

function PreservedContent({ content }: { content: string | null }) {
  if (!content) return null;
  return (
    <details className="preserved-content">
      <summary>Preserved unrecognized source content</summary>
      <MarkdownContent content={content} />
    </details>
  );
}

function MarkdownContent({ content }: { content: string }) {
  return (
    <div className="markdown-body compact">
      <Markdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeRaw, [rehypeSanitize, markdownSanitizeSchema]]}
      >
        {content}
      </Markdown>
    </div>
  );
}

function getPullUnavailableReason(git: GitInfo): string | null {
  if (!git.isRepository) return "Pull is available only for Git repositories";
  if (git.fetchStatus === "fetching") return "Wait for the current Git fetch to finish";
  if (!git.branch || git.branch.startsWith("Detached at ")) return "Pull requires a checked-out branch";
  if (!git.upstream) return "Pull requires an upstream branch";
  if (git.dirty !== false) return "Pull requires a clean working tree";
  return null;
}

function CenteredMessage({ title, body, action }: { title: string; body?: string; action?: React.ReactNode }) {
  return <div className="centered-message"><div className="projector-mark">P</div><h2>{title}</h2>{body && <p>{body}</p>}{action}</div>;
}

function StatusMessage({ title, body, tone = "default" }: { title: string; body?: string; tone?: "default" | "error" }) {
  return <div className={`status-message ${tone}`}><h3>{title}</h3>{body && <p>{body}</p>}</div>;
}

function formatDate(value: string | null): string {
  if (!value) return "Not available";
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return "Not available";
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(date);
}

function formatLocalDateTime(value: string): string {
  const normalized = value.includes("T") ? value : value.replace(" ", "T");
  const parsed = new Date(normalized);
  return Number.isNaN(parsed.getTime())
    ? value
    : parsed.toLocaleString([], {
        year: "numeric",
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      });
}

function todoStatus(dependencies: string[]): "planned" | "blocked" {
  return dependencies.length > 0 ? "blocked" : "planned";
}

function formatRelative(value: string | null): string {
  if (!value) return "No activity";
  const date = new Date(value);
  const seconds = Math.round((date.valueOf() - Date.now()) / 1000);
  if (!Number.isFinite(seconds)) return "No activity";
  const formatter = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
  const ranges: Array<[number, Intl.RelativeTimeFormatUnit]> = [[60, "second"], [60, "minute"], [24, "hour"], [7, "day"], [4.345, "week"], [12, "month"], [Infinity, "year"]];
  let duration = seconds;
  for (const [amount, unit] of ranges) {
    if (Math.abs(duration) < amount) return formatter.format(Math.round(duration), unit);
    duration /= amount;
  }
  return formatter.format(Math.round(duration), "year");
}

function messageFrom(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}
