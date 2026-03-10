#![allow(dead_code)]

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::connection::open_connection;
use crate::errors::{GroveError, Result};
use crate::types::{
    AgentSession, AgentState, InsertRun, ListRunsOpts, PurgeSessionOpts, Run, RunStatus,
};

const SESSIONS_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY,
  agent_name TEXT NOT NULL UNIQUE,
  capability TEXT NOT NULL,
  worktree_path TEXT NOT NULL,
  branch_name TEXT NOT NULL,
  task_id TEXT NOT NULL,
  tmux_session TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'booting'
    CHECK(state IN ('booting','working','completed','stalled','zombie')),
  pid INTEGER,
  parent_agent TEXT,
  depth INTEGER NOT NULL DEFAULT 0,
  run_id TEXT,
  started_at TEXT NOT NULL,
  last_activity TEXT NOT NULL,
  escalation_level INTEGER NOT NULL DEFAULT 0,
  stalled_since TEXT,
  transcript_path TEXT
);
CREATE INDEX IF NOT EXISTS idx_sessions_state ON sessions(state);
CREATE INDEX IF NOT EXISTS idx_sessions_run ON sessions(run_id);

CREATE TABLE IF NOT EXISTS runs (
  id TEXT PRIMARY KEY,
  started_at TEXT NOT NULL,
  completed_at TEXT,
  agent_count INTEGER NOT NULL DEFAULT 0,
  coordinator_session_id TEXT,
  status TEXT NOT NULL DEFAULT 'active'
    CHECK(status IN ('active','completed','failed'))
);
CREATE INDEX IF NOT EXISTS idx_runs_status ON runs(status);
";

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentSession> {
    Ok(AgentSession {
        id: row.get(0)?,
        agent_name: row.get(1)?,
        capability: row.get(2)?,
        worktree_path: row.get(3)?,
        branch_name: row.get(4)?,
        task_id: row.get(5)?,
        tmux_session: row.get(6)?,
        state: row.get(7)?,
        pid: row.get(8)?,
        parent_agent: row.get(9)?,
        depth: row.get(10)?,
        run_id: row.get(11)?,
        started_at: row.get(12)?,
        last_activity: row.get(13)?,
        escalation_level: row.get(14)?,
        stalled_since: row.get(15)?,
        transcript_path: row.get(16)?,
    })
}

fn row_to_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<Run> {
    Ok(Run {
        id: row.get(0)?,
        started_at: row.get(1)?,
        completed_at: row.get(2)?,
        agent_count: row.get(3)?,
        coordinator_session_id: row.get(4)?,
        status: row.get(5)?,
    })
}

// ---------------------------------------------------------------------------
// SessionStore
// ---------------------------------------------------------------------------

pub struct SessionStore {
    conn: Connection,
}

impl SessionStore {
    pub fn new(path: &str) -> Result<Self> {
        let conn = open_connection(path)?;
        conn.execute_batch(SESSIONS_SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn upsert(&self, session: &AgentSession) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sessions (
                id, agent_name, capability, worktree_path, branch_name, task_id,
                tmux_session, state, pid, parent_agent, depth, run_id,
                started_at, last_activity, escalation_level, stalled_since, transcript_path
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
            ON CONFLICT(agent_name) DO UPDATE SET
                id = excluded.id,
                capability = excluded.capability,
                worktree_path = excluded.worktree_path,
                branch_name = excluded.branch_name,
                task_id = excluded.task_id,
                tmux_session = excluded.tmux_session,
                state = excluded.state,
                pid = excluded.pid,
                parent_agent = excluded.parent_agent,
                depth = excluded.depth,
                run_id = excluded.run_id,
                started_at = excluded.started_at,
                last_activity = excluded.last_activity,
                escalation_level = excluded.escalation_level,
                stalled_since = excluded.stalled_since,
                transcript_path = excluded.transcript_path",
            params![
                session.id,
                session.agent_name,
                session.capability,
                session.worktree_path,
                session.branch_name,
                session.task_id,
                session.tmux_session,
                session.state,
                session.pid,
                session.parent_agent,
                session.depth,
                session.run_id,
                session.started_at,
                session.last_activity,
                session.escalation_level,
                session.stalled_since,
                session.transcript_path,
            ],
        )?;
        Ok(())
    }

    pub fn get_by_name(&self, agent_name: &str) -> Result<Option<AgentSession>> {
        let result = self
            .conn
            .query_row(
                "SELECT id,agent_name,capability,worktree_path,branch_name,task_id,
                        tmux_session,state,pid,parent_agent,depth,run_id,
                        started_at,last_activity,escalation_level,stalled_since,transcript_path
                 FROM sessions WHERE agent_name = ?1",
                params![agent_name],
                row_to_session,
            )
            .optional()?;
        Ok(result)
    }

    pub fn get_active(&self) -> Result<Vec<AgentSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT id,agent_name,capability,worktree_path,branch_name,task_id,
                    tmux_session,state,pid,parent_agent,depth,run_id,
                    started_at,last_activity,escalation_level,stalled_since,transcript_path
             FROM sessions WHERE state IN ('booting','working','stalled')",
        )?;
        let rows = stmt.query_map([], row_to_session)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(GroveError::from)
    }

    pub fn get_all(&self) -> Result<Vec<AgentSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT id,agent_name,capability,worktree_path,branch_name,task_id,
                    tmux_session,state,pid,parent_agent,depth,run_id,
                    started_at,last_activity,escalation_level,stalled_since,transcript_path
             FROM sessions ORDER BY started_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_session)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(GroveError::from)
    }

    pub fn count(&self) -> Result<i64> {
        let n = self
            .conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
        Ok(n)
    }

    pub fn get_by_run(&self, run_id: &str) -> Result<Vec<AgentSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT id,agent_name,capability,worktree_path,branch_name,task_id,
                    tmux_session,state,pid,parent_agent,depth,run_id,
                    started_at,last_activity,escalation_level,stalled_since,transcript_path
             FROM sessions WHERE run_id = ?1",
        )?;
        let rows = stmt.query_map(params![run_id], row_to_session)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(GroveError::from)
    }

    pub fn update_state(&self, agent_name: &str, state: AgentState) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET state = ?1 WHERE agent_name = ?2",
            params![state, agent_name],
        )?;
        Ok(())
    }

    pub fn update_last_activity(&self, agent_name: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE sessions SET last_activity = ?1 WHERE agent_name = ?2",
            params![now, agent_name],
        )?;
        Ok(())
    }

    pub fn update_escalation(
        &self,
        agent_name: &str,
        level: i32,
        stalled_since: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET escalation_level = ?1, stalled_since = ?2 WHERE agent_name = ?3",
            params![level, stalled_since, agent_name],
        )?;
        Ok(())
    }

    pub fn update_transcript_path(&self, agent_name: &str, path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET transcript_path = ?1 WHERE agent_name = ?2",
            params![path, agent_name],
        )?;
        Ok(())
    }

    pub fn remove(&self, agent_name: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM sessions WHERE agent_name = ?1",
            params![agent_name],
        )?;
        Ok(())
    }

    pub fn purge(&self, opts: PurgeSessionOpts) -> Result<i64> {
        let n = if opts.all {
            self.conn.execute("DELETE FROM sessions", [])?
        } else if let Some(agent) = &opts.agent {
            self.conn
                .execute("DELETE FROM sessions WHERE agent_name = ?1", params![agent])?
        } else if let Some(state) = &opts.state {
            self.conn
                .execute("DELETE FROM sessions WHERE state = ?1", params![state])?
        } else {
            0
        };
        Ok(n as i64)
    }
}

// ---------------------------------------------------------------------------
// RunStore
// ---------------------------------------------------------------------------

pub struct RunStore {
    conn: Connection,
}

impl RunStore {
    pub fn new(path: &str) -> Result<Self> {
        let conn = open_connection(path)?;
        conn.execute_batch(SESSIONS_SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn create_run(&self, run: &InsertRun) -> Result<()> {
        self.conn.execute(
            "INSERT INTO runs (id, started_at, coordinator_session_id, status)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                run.id,
                run.started_at,
                run.coordinator_session_id,
                run.status
            ],
        )?;
        Ok(())
    }

    pub fn get_run(&self, id: &str) -> Result<Option<Run>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, started_at, completed_at, agent_count, coordinator_session_id, status
                 FROM runs WHERE id = ?1",
                params![id],
                row_to_run,
            )
            .optional()?;
        Ok(result)
    }

    pub fn get_active_run(&self) -> Result<Option<Run>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, started_at, completed_at, agent_count, coordinator_session_id, status
                 FROM runs WHERE status = 'active' ORDER BY started_at DESC LIMIT 1",
                [],
                row_to_run,
            )
            .optional()?;
        Ok(result)
    }

    pub fn list_runs(&self, opts: Option<ListRunsOpts>) -> Result<Vec<Run>> {
        let opts = opts.unwrap_or_default();
        let mut sql = String::from(
            "SELECT id, started_at, completed_at, agent_count, coordinator_session_id, status
             FROM runs",
        );
        if let Some(ref status) = opts.status {
            // We'll handle filtering in Rust since building dynamic SQL with rusqlite is verbose
            let _ = status; // will filter below
        }
        sql.push_str(" ORDER BY started_at DESC");
        if let Some(limit) = opts.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_run)?;
        let runs: Vec<Run> = rows.collect::<rusqlite::Result<Vec<_>>>()?;

        // Apply status filter in Rust
        let filtered = if let Some(status) = opts.status {
            runs.into_iter().filter(|r| r.status == status).collect()
        } else {
            runs
        };
        Ok(filtered)
    }

    pub fn increment_agent_count(&self, run_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE runs SET agent_count = agent_count + 1 WHERE id = ?1",
            params![run_id],
        )?;
        Ok(())
    }

    pub fn complete_run(&self, run_id: &str, status: RunStatus) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE runs SET status = ?1, completed_at = ?2 WHERE id = ?3",
            params![status, now, run_id],
        )?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AgentState;

    fn make_session(name: &str) -> AgentSession {
        AgentSession {
            id: uuid::Uuid::new_v4().to_string(),
            agent_name: name.to_string(),
            capability: "builder".to_string(),
            worktree_path: "/tmp/wt".to_string(),
            branch_name: "test-branch".to_string(),
            task_id: "task-001".to_string(),
            tmux_session: "tmux-session".to_string(),
            state: AgentState::Booting,
            pid: Some(1234),
            parent_agent: None,
            depth: 0,
            run_id: Some("run-001".to_string()),
            started_at: "2024-01-01T00:00:00Z".to_string(),
            last_activity: "2024-01-01T00:00:00Z".to_string(),
            escalation_level: 0,
            stalled_since: None,
            transcript_path: None,
        }
    }

    #[test]
    fn test_sessions_schema_idempotent() {
        let store = SessionStore::new(":memory:").unwrap();
        // Opening again should not fail
        drop(store);
        SessionStore::new(":memory:").unwrap();
    }

    #[test]
    fn test_upsert_and_get() {
        let store = SessionStore::new(":memory:").unwrap();
        let session = make_session("agent-a");
        store.upsert(&session).unwrap();

        let fetched = store.get_by_name("agent-a").unwrap().unwrap();
        assert_eq!(fetched.agent_name, "agent-a");
        assert_eq!(fetched.capability, "builder");
        assert_eq!(fetched.state, AgentState::Booting);
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let store = SessionStore::new(":memory:").unwrap();
        let result = store.get_by_name("nobody").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_upsert_updates_existing() {
        let store = SessionStore::new(":memory:").unwrap();
        let session = make_session("agent-b");
        store.upsert(&session).unwrap();

        let mut updated = session.clone();
        updated.state = AgentState::Working;
        store.upsert(&updated).unwrap();

        let fetched = store.get_by_name("agent-b").unwrap().unwrap();
        assert_eq!(fetched.state, AgentState::Working);
    }

    #[test]
    fn test_get_active() {
        let store = SessionStore::new(":memory:").unwrap();
        let s1 = make_session("a1");
        let mut s2 = make_session("a2");
        s2.state = AgentState::Completed;
        store.upsert(&s1).unwrap();
        store.upsert(&s2).unwrap();

        let active = store.get_active().unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].agent_name, "a1");
    }

    #[test]
    fn test_count() {
        let store = SessionStore::new(":memory:").unwrap();
        assert_eq!(store.count().unwrap(), 0);
        store.upsert(&make_session("a1")).unwrap();
        store.upsert(&make_session("a2")).unwrap();
        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn test_update_state() {
        let store = SessionStore::new(":memory:").unwrap();
        store.upsert(&make_session("a1")).unwrap();
        store.update_state("a1", AgentState::Completed).unwrap();
        let s = store.get_by_name("a1").unwrap().unwrap();
        assert_eq!(s.state, AgentState::Completed);
    }

    #[test]
    fn test_update_escalation() {
        let store = SessionStore::new(":memory:").unwrap();
        store.upsert(&make_session("a1")).unwrap();
        store
            .update_escalation("a1", 2, Some("2024-01-01T00:00:00Z"))
            .unwrap();
        let s = store.get_by_name("a1").unwrap().unwrap();
        assert_eq!(s.escalation_level, 2);
        assert_eq!(s.stalled_since, Some("2024-01-01T00:00:00Z".to_string()));
    }

    #[test]
    fn test_remove() {
        let store = SessionStore::new(":memory:").unwrap();
        store.upsert(&make_session("a1")).unwrap();
        store.remove("a1").unwrap();
        assert!(store.get_by_name("a1").unwrap().is_none());
    }

    #[test]
    fn test_purge_all() {
        let store = SessionStore::new(":memory:").unwrap();
        store.upsert(&make_session("a1")).unwrap();
        store.upsert(&make_session("a2")).unwrap();
        let deleted = store
            .purge(PurgeSessionOpts {
                all: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn test_purge_by_state() {
        let store = SessionStore::new(":memory:").unwrap();
        store.upsert(&make_session("a1")).unwrap();
        let mut s2 = make_session("a2");
        s2.state = AgentState::Zombie;
        store.upsert(&s2).unwrap();
        let deleted = store
            .purge(PurgeSessionOpts {
                state: Some(AgentState::Zombie),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(store.count().unwrap(), 1);
    }

    // --- RunStore tests ---

    fn make_run(id: &str) -> InsertRun {
        InsertRun {
            id: id.to_string(),
            started_at: "2024-01-01T00:00:00Z".to_string(),
            coordinator_session_id: None,
            status: RunStatus::Active,
            agent_count: None,
        }
    }

    #[test]
    fn test_run_create_and_get() {
        let store = RunStore::new(":memory:").unwrap();
        store.create_run(&make_run("run-1")).unwrap();
        let run = store.get_run("run-1").unwrap().unwrap();
        assert_eq!(run.id, "run-1");
        assert_eq!(run.status, RunStatus::Active);
        assert_eq!(run.agent_count, 0);
    }

    #[test]
    fn test_get_nonexistent_run() {
        let store = RunStore::new(":memory:").unwrap();
        assert!(store.get_run("nope").unwrap().is_none());
    }

    #[test]
    fn test_get_active_run() {
        let store = RunStore::new(":memory:").unwrap();
        store.create_run(&make_run("run-1")).unwrap();
        let active = store.get_active_run().unwrap().unwrap();
        assert_eq!(active.id, "run-1");
    }

    #[test]
    fn test_increment_agent_count() {
        let store = RunStore::new(":memory:").unwrap();
        store.create_run(&make_run("run-1")).unwrap();
        store.increment_agent_count("run-1").unwrap();
        store.increment_agent_count("run-1").unwrap();
        let run = store.get_run("run-1").unwrap().unwrap();
        assert_eq!(run.agent_count, 2);
    }

    #[test]
    fn test_complete_run() {
        let store = RunStore::new(":memory:").unwrap();
        store.create_run(&make_run("run-1")).unwrap();
        store.complete_run("run-1", RunStatus::Completed).unwrap();
        let run = store.get_run("run-1").unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Completed);
        assert!(run.completed_at.is_some());
    }

    #[test]
    fn test_list_runs() {
        let store = RunStore::new(":memory:").unwrap();
        store.create_run(&make_run("run-1")).unwrap();
        store.create_run(&make_run("run-2")).unwrap();
        let runs = store.list_runs(None).unwrap();
        assert_eq!(runs.len(), 2);
    }

    #[test]
    fn test_list_runs_with_limit() {
        let store = RunStore::new(":memory:").unwrap();
        store.create_run(&make_run("run-1")).unwrap();
        store.create_run(&make_run("run-2")).unwrap();
        let runs = store
            .list_runs(Some(ListRunsOpts {
                limit: Some(1),
                ..Default::default()
            }))
            .unwrap();
        assert_eq!(runs.len(), 1);
    }
}
