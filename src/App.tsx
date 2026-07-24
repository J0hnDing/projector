import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import rehypeRaw from "rehype-raw";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";

import * as api from "./api";
import type {
  GitInfo,
  ProjectDetail,
  ProjectDocument,
  ProjectSummary,
  TodoDocument,
  ValidationWarning,
  WorkHistoryDocument,
  WorkHistoryEntry,
} from "./types";

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
  const [pulling, setPulling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const selectedIdRef = useRef<string | null>(null);
  const refreshTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

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
            pulling={pulling}
            onTab={setTab}
            onRefresh={() => void refresh()}
            onPull={() => void pull()}
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
        {project.git.isRepository && <span>{syncStatusLabel(project.git)}</span>}
        <span className="dot">·</span>
        <span>{formatRelative(project.git.lastActivity ?? project.lastOpened)}</span>
      </span>
    </button>
  );
}

function ProjectView({ detail, tab, refreshing, pulling, onTab, onRefresh, onPull, onRemove }: {
  detail: ProjectDetail;
  tab: Tab;
  refreshing: boolean;
  pulling: boolean;
  onTab: (tab: Tab) => void;
  onRefresh: () => void;
  onPull: () => void;
  onRemove: () => void;
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
          </button>
        ))}
      </div>

      <section className="tab-content">
        {tab === "overview" && <Overview detail={detail} />}
        {tab === "readme" && <DocumentPanel document={detail.documents.readme} />}
        {tab === "todo" && (
          <TodoPanel document={detail.state.todos} source={detail.documents.todo} />
        )}
        {tab === "history" && (
          <HistoryPanel
            document={detail.state.workingHistory}
            source={detail.documents.workingHistory}
            onTodo={(todoId) => {
              onTab("todo");
              requestAnimationFrame(() => {
                document.getElementById(`todo-${todoId}`)?.scrollIntoView({ block: "center" });
              });
            }}
          />
        )}
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

export function TodoPanel({
  document,
  source,
}: {
  document: TodoDocument;
  source: ProjectDocument;
}) {
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

  const warningIds = new Set(
    document.warnings
      .filter((warning) =>
        ["missing_dependency", "circular_dependency"].includes(warning.code),
      )
      .map((warning) => warning.itemId)
      .filter(Boolean),
  );

  return (
    <div className="structured-panel">
      <StructuredHeader
        path={document.relativePath}
        count={document.items.length}
        noun="TODO"
      />
      <ValidationWarnings warnings={document.warnings} />
      {document.items.length ? (
        <>
          <section className="dependency-map panel" aria-label="TODO dependency relationships">
            <p className="section-label">Dependencies</p>
            <div className="dependency-rows">
              {document.items.map((item) => (
                <div
                  className={`dependency-row${warningIds.has(item.id) ? " invalid" : ""}`}
                  key={item.id}
                >
                  <code>{item.id}</code>
                  <span aria-hidden="true">→</span>
                  <span>
                    {item.dependencies.length
                      ? item.dependencies.join(", ")
                      : "ready (no dependencies)"}
                  </span>
                </div>
              ))}
            </div>
          </section>
          <div className="todo-list">
            {document.items.map((item) => (
              <article
                className={`todo-card priority-${item.priority}${item.status === "blocked" ? " blocked" : ""}`}
                id={`todo-${item.id}`}
                key={item.id}
              >
                <div className="todo-card-heading">
                  <div>
                    <code>{item.id}</code>
                    <h3>{item.title}</h3>
                  </div>
                  <div className="tag-row">
                    <span className={`tag priority ${item.priority}`}>{item.priority}</span>
                    <span className={`tag status ${item.status}`}>{item.status}</span>
                    <span className="tag">{item.area}</span>
                  </div>
                </div>
                <dl className="todo-details">
                  <div>
                    <dt>Depends on</dt>
                    <dd>{item.dependencies.length ? item.dependencies.join(", ") : "none"}</dd>
                  </div>
                  <div>
                    <dt>Rationale</dt>
                    <dd>{item.rationale}</dd>
                  </div>
                </dl>
                <div className="criteria">
                  <p className="section-label">Acceptance criteria</p>
                  <MarkdownContent content={item.acceptanceCriteria} />
                </div>
              </article>
            ))}
          </div>
        </>
      ) : (
        <StatusMessage title="No open TODOs" body="This project has no structured unfinished work." />
      )}
      <PreservedContent content={document.preservedContent} />
    </div>
  );
}

export function HistoryPanel({
  document,
  source,
  onTodo,
}: {
  document: WorkHistoryDocument;
  source: ProjectDocument;
  onTodo: (todoId: string) => void;
}) {
  const [category, setCategory] = useState("all");
  const [area, setArea] = useState("all");
  const [relatedTodo, setRelatedTodo] = useState("");
  const [groupBy, setGroupBy] = useState<"category" | "area">("category");

  if (source.status === "missing") {
    return (
      <StatusMessage
        title="WORK_HISTORY.md was not found"
        body="Projector's add_work_history operation can create the first structured entry."
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

  const normalizedTodo = relatedTodo.trim().toUpperCase();
  const filtered = document.entries.filter(
    (entry) =>
      (category === "all" || entry.category === category) &&
      (area === "all" || entry.area === area) &&
      (!normalizedTodo || entry.relatedTodos.includes(normalizedTodo)),
  );
  const grouped = filtered.reduce<Map<string, WorkHistoryEntry[]>>((groups, entry) => {
    const key = groupBy === "category" ? entry.category : entry.area;
    groups.set(key, [...(groups.get(key) ?? []), entry]);
    return groups;
  }, new Map());

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
        <label>
          Related TODO
          <input
            value={relatedTodo}
            onChange={(event) => setRelatedTodo(event.target.value)}
            placeholder="TODO-001"
          />
        </label>
        <label>
          Group by
          <select
            value={groupBy}
            onChange={(event) => setGroupBy(event.target.value as "category" | "area")}
          >
            <option value="category">Category</option>
            <option value="area">Area</option>
          </select>
        </label>
      </div>
      {grouped.size ? (
        [...grouped.entries()].map(([group, entries]) => (
          <section className="history-group" key={group}>
            <h3>{group}</h3>
            <div className="history-list">
              {entries.map((entry) => (
                <article
                  className="history-card"
                  key={`${entry.occurredAt}-${entry.title}`}
                >
                  <div className="history-heading">
                    <div>
                      <time>{formatLocalDateTime(entry.occurredAt)}</time>
                      <h4>{entry.title}</h4>
                    </div>
                    <div className="tag-row">
                      <span className="tag category">{entry.category}</span>
                      <span className="tag">{entry.area}</span>
                    </div>
                  </div>
                  {entry.relatedTodos.length > 0 && (
                    <div className="related-todos">
                      Related:
                      {entry.relatedTodos.map((todoId) => (
                        <button key={todoId} onClick={() => onTodo(todoId)}>
                          {todoId}
                        </button>
                      ))}
                    </div>
                  )}
                  <section>
                    <p className="section-label">Summary</p>
                    <MarkdownContent content={entry.summary} />
                  </section>
                  <section>
                    <p className="section-label">Limitations</p>
                    <MarkdownContent content={entry.limitations} />
                  </section>
                </article>
              ))}
            </div>
          </section>
        ))
      ) : (
        <StatusMessage title="No matching history" body="Adjust the filters to show entries." />
      )}
      <PreservedContent content={document.preservedContent} />
    </div>
  );
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
