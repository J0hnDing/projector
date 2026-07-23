import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import rehypeRaw from "rehype-raw";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";

import * as api from "./api";
import type { GitInfo, ProjectDetail, ProjectDocument, ProjectSummary } from "./types";

type Tab = "overview" | "readme" | "todo" | "history" | "git";

const tabs: Array<{ id: Tab; label: string }> = [
  { id: "overview", label: "Overview" },
  { id: "readme", label: "README" },
  { id: "todo", label: "TODO" },
  { id: "history", label: "Working history" },
  { id: "git", label: "Git activity" },
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
  const [error, setError] = useState<string | null>(null);
  const selectedIdRef = useRef<string | null>(null);
  const refreshTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    selectedIdRef.current = selectedId;
  }, [selectedId]);

  const openProject = useCallback(async (id: string, updateOpened = true) => {
    setSelectedId(id);
    selectedIdRef.current = id;
    setError(null);
    setRefreshing(true);
    try {
      const nextDetail = updateOpened
        ? await api.openProject(id)
        : await api.refreshProject(id);
      setDetail(nextDetail);
    } catch (reason) {
      setDetail(null);
      setError(messageFrom(reason));
    } finally {
      setRefreshing(false);
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
            if (id) await openProject(id, false);
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
      await loadProjects();
      if (selectedIdRef.current) await openProject(selectedIdRef.current, false);
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

        <button className="primary-button register-button" onClick={() => void register()}>
          <span aria-hidden="true">＋</span> Register project
        </button>

        <div className="project-count">
          {projects.length} {projects.length === 1 ? "project" : "projects"}
        </div>
        <nav className="project-list" aria-label="Registered projects">
          {projects.map((project) => (
            <ProjectListItem
              key={project.id}
              project={project}
              selected={project.id === selectedId}
              onSelect={() => void openProject(project.id)}
            />
          ))}
        </nav>
        {!loading && projects.length === 0 && (
          <div className="sidebar-empty">
            Register a local folder to see its documentation and Git activity.
          </div>
        )}
      </aside>

      <main className="workspace">
        {error && (
          <div className="error-banner" role="alert">
            <span>{error}</span>
            <button onClick={() => setError(null)} aria-label="Dismiss error">×</button>
          </div>
        )}
        {loading ? (
          <CenteredMessage title="Loading projects…" />
        ) : detail ? (
          <ProjectView
            detail={detail}
            tab={tab}
            refreshing={refreshing}
            onTab={setTab}
            onRefresh={() => void refresh()}
            onRemove={() => void remove()}
          />
        ) : (
          <CenteredMessage
            title={projects.length ? "Select a project" : "Your projects, at a glance"}
            body={projects.length ? "Choose a registered project from the list." : "Register a local project directory to get started. Projector only observes files; it never runs your project."}
            action={!projects.length ? <button className="primary-button" onClick={() => void register()}>Register your first project</button> : undefined}
          />
        )}
      </main>
    </div>
  );
}

function ProjectListItem({ project, selected, onSelect }: { project: ProjectSummary; selected: boolean; onSelect: () => void }) {
  return (
    <button className={`project-list-item${selected ? " selected" : ""}`} onClick={onSelect}>
      <span className="project-list-name">{project.name}</span>
      <span className="project-list-path" title={project.path}>{project.path}</span>
      <span className="project-list-meta">
        <GitBadge git={project.git} />
        <span>{project.git.branch ?? "No Git"}</span>
        <span className="dot">·</span>
        <span>{formatRelative(project.git.lastActivity ?? project.lastOpened)}</span>
      </span>
    </button>
  );
}

function ProjectView({ detail, tab, refreshing, onTab, onRefresh, onRemove }: {
  detail: ProjectDetail;
  tab: Tab;
  refreshing: boolean;
  onTab: (tab: Tab) => void;
  onRefresh: () => void;
  onRemove: () => void;
}) {
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
          </button>
        ))}
      </div>

      <section className="tab-content">
        {tab === "overview" && <Overview detail={detail} />}
        {tab === "readme" && <DocumentPanel document={detail.documents.readme} />}
        {tab === "todo" && <DocumentPanel document={detail.documents.todo} />}
        {tab === "history" && <DocumentPanel document={detail.documents.workingHistory} />}
        {tab === "git" && <GitPanel git={detail.project.git} />}
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
          <div className="fact-grid">
            <Fact label="Branch" value={detail.project.git.branch ?? "Unknown"} />
            <Fact label="Working tree" value={detail.project.git.dirty ? "Uncommitted changes" : "Clean"} />
            <Fact label="Last activity" value={formatDate(detail.project.git.lastActivity)} />
            <Fact label="Last opened" value={formatDate(detail.project.lastOpened)} />
          </div>
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

export function DocumentPanel({ document }: { document: ProjectDocument }) {
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
          remarkPlugins={[remarkGfm]}
          rehypePlugins={[rehypeRaw, [rehypeSanitize, markdownSanitizeSchema]]}
        >
          {document.content ?? ""}
        </Markdown>
      </div>
    </article>
  );
}

function GitPanel({ git }: { git: GitInfo }) {
  if (!git.isRepository) {
    return <StatusMessage title="Not a Git repository" body={git.error ?? "This project can still be observed through its Markdown files."} />;
  }
  return (
    <div className="git-panel">
      {git.error && <div className="notice error">{git.error}</div>}
      <div className="fact-grid panel compact">
        <Fact label="Branch" value={git.branch ?? "Unknown"} />
        <Fact label="Working tree" value={git.dirty === null ? "Unavailable" : git.dirty ? "Uncommitted changes" : "Clean"} />
        <Fact label="Last activity" value={formatDate(git.lastActivity)} />
        <Fact label="Commits shown" value={String(git.recentCommits.length)} />
      </div>
      <section className="panel commits-panel">
        <p className="section-label">Recent commits</p>
        {git.recentCommits.length ? <CommitList commits={git.recentCommits} /> : <StatusMessage title="No commits to show" />}
      </section>
    </div>
  );
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
