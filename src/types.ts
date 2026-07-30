export type DocumentStatus = "available" | "missing" | "error";

export interface GitCommit {
  id: string;
  summary: string;
  author: string;
  committedAt: string;
}

export interface GitInfo {
  isRepository: boolean;
  branch: string | null;
  dirty: boolean | null;
  recentCommits: GitCommit[];
  lastActivity: string | null;
  upstream: string | null;
  ahead: number | null;
  behind: number | null;
  syncStatus: "ahead" | "behind" | "diverged" | "synchronized" | "unknown";
  syncMessage: string | null;
  fetchStatus: "idle" | "fetching" | "succeeded" | "noRemote" | "authenticationFailed" | "offline" | "timedOut" | "failed";
  lastSuccessfulFetch: string | null;
  fetchError: string | null;
  error: string | null;
}

export interface ProjectSummary {
  id: string;
  name: string;
  path: string;
  registeredAt: string;
  lastOpened: string | null;
  git: GitInfo;
}

export interface ProjectDocument {
  name: string;
  relativePath: string | null;
  status: DocumentStatus;
  content: string | null;
  modifiedAt: string | null;
  truncated: boolean;
  error: string | null;
}

export interface ProjectDocuments {
  readme: ProjectDocument;
  todo: ProjectDocument;
  workingHistory: ProjectDocument;
}

export interface ProjectDetail {
  project: ProjectSummary;
  documents: ProjectDocuments;
  state: ProjectState;
}

export type TodoPriority = "critical" | "high" | "medium" | "low";
export type WorkCategory =
  | "feature"
  | "bugfix"
  | "refactor"
  | "test"
  | "documentation"
  | "research"
  | "others";

export interface ValidationWarning {
  code: string;
  message: string;
  itemId: string | null;
}

export interface TodoItem {
  id: string;
  title: string;
  priority: TodoPriority;
  category: WorkCategory;
  area: string;
  dependencies: string[];
  rationale: string;
  acceptanceCriteria: string;
}

export interface TodoDocument {
  relativePath: string | null;
  items: TodoItem[];
  warnings: ValidationWarning[];
  preservedContent: string | null;
}

export interface WorkHistoryEntry {
  occurredAt: string;
  title: string;
  category: WorkCategory;
  area: string;
  summary: string;
  limitations: string;
}

export interface CompletionProposal {
  id: string;
  projectId: string;
  requestedAt: string;
  kind: "todoCompletion" | "workHistory";
  todo: TodoItem | null;
  proposedEntry: WorkHistoryEntry;
}

export interface WorkHistoryDocument {
  relativePath: string | null;
  entries: WorkHistoryEntry[];
  categories: WorkCategory[];
  areas: string[];
  warnings: ValidationWarning[];
  preservedContent: string | null;
}

export interface ProjectState {
  todos: TodoDocument;
  workingHistory: WorkHistoryDocument;
  pendingReviews: CompletionProposal[];
}

