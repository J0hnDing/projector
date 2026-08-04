use std::{fs, path::PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

const STORE_VERSION: u8 = 1;
const MAX_UNLINKED_SESSIONS: usize = 50;
const MAX_TRANSITIONS: usize = 100;
const MAX_ID_LENGTH: usize = 256;
const MAX_CWD_LENGTH: usize = 4096;
const MAX_AGENT_TYPE_LENGTH: usize = 256;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LifecycleState {
    Running,
    Stopped,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexAgent {
    pub agent_id: String,
    pub agent_type: String,
    pub state: LifecycleState,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexTransition {
    pub kind: TransitionKind,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TransitionKind {
    SessionStarted,
    SessionStopped,
    SessionUnknown,
    SubagentStarted,
    SubagentStopped,
    SubagentUnknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSession {
    pub session_id: String,
    pub cwd: String,
    pub linked_project_id: Option<Uuid>,
    pub state: LifecycleState,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub agents: Vec<CodexAgent>,
    pub transitions: Vec<CodexTransition>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexMonitoringSnapshot {
    pub detected_sessions: Vec<CodexSession>,
    pub linked_sessions: Vec<CodexSession>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "hook_event_name")]
pub enum CodexHookEvent {
    SessionStart {
        session_id: String,
        cwd: String,
    },
    SessionEnd {
        session_id: String,
        cwd: String,
    },
    SubagentStart {
        session_id: String,
        cwd: String,
        agent_id: String,
        agent_type: String,
    },
    SubagentStop {
        session_id: String,
        cwd: String,
        agent_id: String,
        agent_type: String,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct CodexMonitorData {
    #[serde(default = "store_version")]
    version: u8,
    #[serde(default)]
    sessions: Vec<CodexSession>,
}

fn store_version() -> u8 {
    STORE_VERSION
}

#[derive(Debug, Error)]
pub enum CodexMonitorError {
    #[error("Unable to read or write Codex monitoring data: {0}")]
    Io(#[from] std::io::Error),
    #[error("The saved Codex monitoring data is invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Unsupported Codex monitoring data version: {0}")]
    UnsupportedVersion(u8),
    #[error("The Codex hook event is invalid: {0}")]
    Validation(String),
    #[error("Unknown detected Codex session {0}")]
    UnknownSession(String),
    #[error("Codex session {session_id} is already linked to another project")]
    AlreadyLinked { session_id: String },
}

pub struct CodexMonitorStore {
    file_path: PathBuf,
    data: CodexMonitorData,
}

impl CodexMonitorStore {
    pub fn load(file_path: PathBuf) -> Result<Self, CodexMonitorError> {
        let mut data = if file_path.exists() {
            serde_json::from_slice::<CodexMonitorData>(&fs::read(&file_path)?)?
        } else {
            CodexMonitorData {
                version: STORE_VERSION,
                sessions: Vec::new(),
            }
        };
        if data.version != STORE_VERSION {
            return Err(CodexMonitorError::UnsupportedVersion(data.version));
        }

        let now = Utc::now();
        let mut changed = false;
        for session in &mut data.sessions {
            if session.state == LifecycleState::Running {
                session.state = LifecycleState::Unknown;
                push_transition(session, TransitionKind::SessionUnknown, None, None, now);
                changed = true;
            }
            let mut unknown_agents = Vec::new();
            for agent in &mut session.agents {
                if agent.state == LifecycleState::Running {
                    agent.state = LifecycleState::Unknown;
                    unknown_agents.push((agent.agent_id.clone(), agent.agent_type.clone()));
                    changed = true;
                }
            }
            for (agent_id, agent_type) in unknown_agents {
                push_transition(
                    session,
                    TransitionKind::SubagentUnknown,
                    Some(agent_id),
                    Some(agent_type),
                    now,
                );
            }
        }

        let store = Self { file_path, data };
        if changed {
            store.persist()?;
        }
        Ok(store)
    }

    pub fn ingest(&mut self, event: CodexHookEvent) -> Result<(), CodexMonitorError> {
        validate_event(&event)?;
        let now = Utc::now();
        match event {
            CodexHookEvent::SessionStart { session_id, cwd } => {
                let session = self.session_mut(&session_id, &cwd, now, LifecycleState::Running);
                if session.state != LifecycleState::Running {
                    session.state = LifecycleState::Running;
                    push_transition(session, TransitionKind::SessionStarted, None, None, now);
                }
            }
            CodexHookEvent::SessionEnd { session_id, cwd } => {
                let session = self.session_mut(&session_id, &cwd, now, LifecycleState::Stopped);
                if session.state != LifecycleState::Stopped {
                    session.state = LifecycleState::Stopped;
                    push_transition(session, TransitionKind::SessionStopped, None, None, now);
                }
                let mut unknown_agents = Vec::new();
                for agent in &mut session.agents {
                    if agent.state == LifecycleState::Running {
                        agent.state = LifecycleState::Unknown;
                        agent.last_seen_at = now;
                        unknown_agents.push((agent.agent_id.clone(), agent.agent_type.clone()));
                    }
                }
                for (agent_id, agent_type) in unknown_agents {
                    push_transition(
                        session,
                        TransitionKind::SubagentUnknown,
                        Some(agent_id),
                        Some(agent_type),
                        now,
                    );
                }
            }
            CodexHookEvent::SubagentStart {
                session_id,
                cwd,
                agent_id,
                agent_type,
            } => {
                let session = self.session_mut(&session_id, &cwd, now, LifecycleState::Running);
                if session.state != LifecycleState::Stopped {
                    session.state = LifecycleState::Running;
                }
                match session
                    .agents
                    .iter_mut()
                    .find(|agent| agent.agent_id == agent_id)
                {
                    Some(agent) if agent.state == LifecycleState::Stopped => {}
                    Some(agent) => {
                        agent.last_seen_at = now;
                        if agent.state != LifecycleState::Running {
                            agent.state = LifecycleState::Running;
                            push_transition(
                                session,
                                TransitionKind::SubagentStarted,
                                Some(agent_id),
                                Some(agent_type),
                                now,
                            );
                        }
                    }
                    None => {
                        session.agents.push(CodexAgent {
                            agent_id: agent_id.clone(),
                            agent_type: agent_type.clone(),
                            state: LifecycleState::Running,
                            first_seen_at: now,
                            last_seen_at: now,
                        });
                        push_transition(
                            session,
                            TransitionKind::SubagentStarted,
                            Some(agent_id),
                            Some(agent_type),
                            now,
                        );
                    }
                }
            }
            CodexHookEvent::SubagentStop {
                session_id,
                cwd,
                agent_id,
                agent_type,
            } => {
                let session = self.session_mut(&session_id, &cwd, now, LifecycleState::Unknown);
                match session
                    .agents
                    .iter_mut()
                    .find(|agent| agent.agent_id == agent_id)
                {
                    Some(agent) => {
                        agent.last_seen_at = now;
                        if agent.state != LifecycleState::Stopped {
                            agent.state = LifecycleState::Stopped;
                            push_transition(
                                session,
                                TransitionKind::SubagentStopped,
                                Some(agent_id),
                                Some(agent_type),
                                now,
                            );
                        }
                    }
                    None => {
                        session.agents.push(CodexAgent {
                            agent_id: agent_id.clone(),
                            agent_type: agent_type.clone(),
                            state: LifecycleState::Stopped,
                            first_seen_at: now,
                            last_seen_at: now,
                        });
                        push_transition(
                            session,
                            TransitionKind::SubagentStopped,
                            Some(agent_id),
                            Some(agent_type),
                            now,
                        );
                    }
                }
            }
        }
        self.prune_unlinked();
        self.persist()
    }

    pub fn snapshot(&self, project_id: Uuid) -> CodexMonitoringSnapshot {
        let mut detected_sessions: Vec<_> = self
            .data
            .sessions
            .iter()
            .filter(|session| session.linked_project_id.is_none())
            .cloned()
            .collect();
        let mut linked_sessions: Vec<_> = self
            .data
            .sessions
            .iter()
            .filter(|session| session.linked_project_id == Some(project_id))
            .cloned()
            .collect();
        detected_sessions.sort_by_key(|session| std::cmp::Reverse(session.last_seen_at));
        linked_sessions.sort_by_key(|session| std::cmp::Reverse(session.last_seen_at));
        CodexMonitoringSnapshot {
            detected_sessions,
            linked_sessions,
        }
    }

    pub fn link(&mut self, project_id: Uuid, session_id: &str) -> Result<(), CodexMonitorError> {
        let session = self
            .data
            .sessions
            .iter_mut()
            .find(|session| session.session_id == session_id)
            .ok_or_else(|| CodexMonitorError::UnknownSession(session_id.to_string()))?;
        if session
            .linked_project_id
            .is_some_and(|linked| linked != project_id)
        {
            return Err(CodexMonitorError::AlreadyLinked {
                session_id: session_id.to_string(),
            });
        }
        session.linked_project_id = Some(project_id);
        self.persist()
    }

    pub fn unlink(&mut self, project_id: Uuid, session_id: &str) -> Result<(), CodexMonitorError> {
        let session = self
            .data
            .sessions
            .iter_mut()
            .find(|session| {
                session.session_id == session_id && session.linked_project_id == Some(project_id)
            })
            .ok_or_else(|| CodexMonitorError::UnknownSession(session_id.to_string()))?;
        session.linked_project_id = None;
        self.prune_unlinked();
        self.persist()
    }

    pub fn remove_project(&mut self, project_id: Uuid) -> Result<(), CodexMonitorError> {
        let original_len = self.data.sessions.len();
        self.data
            .sessions
            .retain(|session| session.linked_project_id != Some(project_id));
        if self.data.sessions.len() != original_len {
            self.persist()?;
        }
        Ok(())
    }

    fn session_mut(
        &mut self,
        session_id: &str,
        cwd: &str,
        now: DateTime<Utc>,
        initial_state: LifecycleState,
    ) -> &mut CodexSession {
        if let Some(index) = self
            .data
            .sessions
            .iter()
            .position(|session| session.session_id == session_id)
        {
            let session = &mut self.data.sessions[index];
            session.cwd = cwd.to_string();
            session.last_seen_at = now;
            return session;
        }
        self.data.sessions.push(CodexSession {
            session_id: session_id.to_string(),
            cwd: cwd.to_string(),
            linked_project_id: None,
            state: initial_state,
            first_seen_at: now,
            last_seen_at: now,
            agents: Vec::new(),
            transitions: vec![CodexTransition {
                kind: match initial_state {
                    LifecycleState::Running => TransitionKind::SessionStarted,
                    LifecycleState::Stopped => TransitionKind::SessionStopped,
                    LifecycleState::Unknown => TransitionKind::SessionUnknown,
                },
                agent_id: None,
                agent_type: None,
                observed_at: now,
            }],
        });
        self.data.sessions.last_mut().expect("session was inserted")
    }

    fn prune_unlinked(&mut self) {
        let mut unlinked: Vec<_> = self
            .data
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| session.linked_project_id.is_none())
            .map(|(index, session)| (index, session.last_seen_at))
            .collect();
        if unlinked.len() <= MAX_UNLINKED_SESSIONS {
            return;
        }
        unlinked.sort_by_key(|(_, last_seen_at)| *last_seen_at);
        let excess = unlinked.len() - MAX_UNLINKED_SESSIONS;
        let mut remove: Vec<_> = unlinked
            .into_iter()
            .take(excess)
            .map(|(index, _)| index)
            .collect();
        remove.sort_unstable_by(|left, right| right.cmp(left));
        for index in remove {
            self.data.sessions.remove(index);
        }
    }

    fn persist(&self) -> Result<(), CodexMonitorError> {
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.file_path, serde_json::to_vec_pretty(&self.data)?)?;
        Ok(())
    }
}

fn push_transition(
    session: &mut CodexSession,
    kind: TransitionKind,
    agent_id: Option<String>,
    agent_type: Option<String>,
    observed_at: DateTime<Utc>,
) {
    session.transitions.push(CodexTransition {
        kind,
        agent_id,
        agent_type,
        observed_at,
    });
    if session.transitions.len() > MAX_TRANSITIONS {
        let excess = session.transitions.len() - MAX_TRANSITIONS;
        session.transitions.drain(0..excess);
    }
}

fn validate_event(event: &CodexHookEvent) -> Result<(), CodexMonitorError> {
    let (session_id, cwd, agent) = match event {
        CodexHookEvent::SessionStart { session_id, cwd }
        | CodexHookEvent::SessionEnd { session_id, cwd } => (session_id, cwd, None),
        CodexHookEvent::SubagentStart {
            session_id,
            cwd,
            agent_id,
            agent_type,
        }
        | CodexHookEvent::SubagentStop {
            session_id,
            cwd,
            agent_id,
            agent_type,
        } => (session_id, cwd, Some((agent_id, agent_type))),
    };
    validate_text("session_id", session_id, MAX_ID_LENGTH)?;
    validate_text("cwd", cwd, MAX_CWD_LENGTH)?;
    if let Some((agent_id, agent_type)) = agent {
        validate_text("agent_id", agent_id, MAX_ID_LENGTH)?;
        validate_text("agent_type", agent_type, MAX_AGENT_TYPE_LENGTH)?;
    }
    Ok(())
}

fn validate_text(name: &str, value: &str, max: usize) -> Result<(), CodexMonitorError> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(CodexMonitorError::Validation(format!(
            "{name} must be non-empty, contain no control characters, and be at most {max} bytes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    fn start(session_id: &str) -> CodexHookEvent {
        CodexHookEvent::SubagentStart {
            session_id: session_id.into(),
            cwd: "C:\\code\\project".into(),
            agent_id: "agent-1".into(),
            agent_type: "worker_low".into(),
        }
    }

    #[test]
    fn deduplicates_events_and_keeps_stops_terminal() {
        let temp = tempdir().unwrap();
        let mut store = CodexMonitorStore::load(temp.path().join("codex-sessions.json")).unwrap();
        store.ingest(start("session-1")).unwrap();
        store.ingest(start("session-1")).unwrap();
        store
            .ingest(CodexHookEvent::SubagentStop {
                session_id: "session-1".into(),
                cwd: "C:\\code\\project".into(),
                agent_id: "agent-1".into(),
                agent_type: "worker_low".into(),
            })
            .unwrap();
        store.ingest(start("session-1")).unwrap();

        let session = &store.snapshot(Uuid::new_v4()).detected_sessions[0];
        assert_eq!(session.agents[0].state, LifecycleState::Stopped);
        assert_eq!(
            session
                .transitions
                .iter()
                .filter(|transition| transition.kind == TransitionKind::SubagentStarted)
                .count(),
            1
        );
    }

    #[test]
    fn accepts_stop_before_start_without_reopening_agent() {
        let temp = tempdir().unwrap();
        let mut store = CodexMonitorStore::load(temp.path().join("codex-sessions.json")).unwrap();
        store
            .ingest(CodexHookEvent::SubagentStop {
                session_id: "session-1".into(),
                cwd: "C:\\code\\project".into(),
                agent_id: "agent-1".into(),
                agent_type: "worker_low".into(),
            })
            .unwrap();
        store.ingest(start("session-1")).unwrap();
        assert_eq!(
            store.snapshot(Uuid::new_v4()).detected_sessions[0].agents[0].state,
            LifecycleState::Stopped
        );
    }

    #[test]
    fn restart_marks_running_state_unknown_and_omits_sensitive_hook_fields() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("codex-sessions.json");
        let mut store = CodexMonitorStore::load(path.clone()).unwrap();
        let event: CodexHookEvent = serde_json::from_value(json!({
            "hook_event_name": "SubagentStart",
            "session_id": "session-1",
            "cwd": "C:\\code\\project",
            "agent_id": "agent-1",
            "agent_type": "worker_low",
            "transcript_path": "secret-path",
            "last_assistant_message": "secret response",
            "tool_input": {"command": "secret command"}
        }))
        .unwrap();
        store.ingest(event).unwrap();
        let persisted = fs::read_to_string(&path).unwrap();
        assert!(!persisted.contains("secret"));

        let reloaded = CodexMonitorStore::load(path).unwrap();
        let session = &reloaded.snapshot(Uuid::new_v4()).detected_sessions[0];
        assert_eq!(session.state, LifecycleState::Unknown);
        assert_eq!(session.agents[0].state, LifecycleState::Unknown);
    }

    #[test]
    fn links_to_one_project_and_removes_project_data() {
        let temp = tempdir().unwrap();
        let mut store = CodexMonitorStore::load(temp.path().join("codex-sessions.json")).unwrap();
        store.ingest(start("session-1")).unwrap();
        let project_id = Uuid::new_v4();
        store.link(project_id, "session-1").unwrap();
        assert_eq!(store.snapshot(project_id).linked_sessions.len(), 1);
        assert!(matches!(
            store.link(Uuid::new_v4(), "session-1"),
            Err(CodexMonitorError::AlreadyLinked { .. })
        ));
        store.remove_project(project_id).unwrap();
        assert!(store.snapshot(project_id).linked_sessions.is_empty());
    }

    #[test]
    fn validates_identifiers() {
        let temp = tempdir().unwrap();
        let mut store = CodexMonitorStore::load(temp.path().join("codex-sessions.json")).unwrap();
        assert!(matches!(
            store.ingest(CodexHookEvent::SessionStart {
                session_id: "".into(),
                cwd: "C:\\code".into(),
            }),
            Err(CodexMonitorError::Validation(_))
        ));
    }

    #[test]
    fn bounds_unlinked_sessions_and_transition_history() {
        let temp = tempdir().unwrap();
        let mut store = CodexMonitorStore::load(temp.path().join("codex-sessions.json")).unwrap();
        for index in 0..55 {
            store
                .ingest(CodexHookEvent::SessionStart {
                    session_id: format!("session-{index:02}"),
                    cwd: "C:\\code\\project".into(),
                })
                .unwrap();
        }
        let snapshot = store.snapshot(Uuid::new_v4());
        assert_eq!(snapshot.detected_sessions.len(), MAX_UNLINKED_SESSIONS);
        assert!(
            snapshot
                .detected_sessions
                .iter()
                .all(|session| session.session_id != "session-00")
        );

        for index in 0..120 {
            store
                .ingest(CodexHookEvent::SubagentStart {
                    session_id: "session-54".into(),
                    cwd: "C:\\code\\project".into(),
                    agent_id: format!("agent-{index}"),
                    agent_type: "worker_low".into(),
                })
                .unwrap();
        }
        let session = store
            .snapshot(Uuid::new_v4())
            .detected_sessions
            .into_iter()
            .find(|session| session.session_id == "session-54")
            .unwrap();
        assert_eq!(session.transitions.len(), MAX_TRANSITIONS);
    }

    #[test]
    fn serializes_concurrent_ingestion_through_the_store_lock() {
        let temp = tempdir().unwrap();
        let store = Arc::new(Mutex::new(
            CodexMonitorStore::load(temp.path().join("codex-sessions.json")).unwrap(),
        ));
        let handles: Vec<_> = (0..12)
            .map(|index| {
                let store = Arc::clone(&store);
                std::thread::spawn(move || {
                    store
                        .lock()
                        .unwrap()
                        .ingest(CodexHookEvent::SubagentStart {
                            session_id: "session-1".into(),
                            cwd: "C:\\code\\project".into(),
                            agent_id: format!("agent-{index}"),
                            agent_type: "worker_low".into(),
                        })
                        .unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(
            store
                .lock()
                .unwrap()
                .snapshot(Uuid::new_v4())
                .detected_sessions[0]
                .agents
                .len(),
            12
        );
    }
}
