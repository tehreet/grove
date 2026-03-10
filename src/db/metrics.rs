#![allow(dead_code)]

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::connection::open_connection;
use crate::errors::{GroveError, Result};
use crate::types::{PurgeMetricsOpts, PurgeSnapshotOpts, SessionMetrics, TokenSnapshot};

const METRICS_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sessions (
  agent_name TEXT NOT NULL,
  task_id TEXT NOT NULL,
  capability TEXT NOT NULL,
  started_at TEXT NOT NULL,
  completed_at TEXT,
  duration_ms INTEGER NOT NULL DEFAULT 0,
  exit_code INTEGER,
  merge_result TEXT,
  parent_agent TEXT,
  input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  cache_read_tokens INTEGER NOT NULL DEFAULT 0,
  cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
  estimated_cost_usd REAL,
  model_used TEXT,
  run_id TEXT,
  PRIMARY KEY (agent_name, task_id)
);

CREATE TABLE IF NOT EXISTS token_snapshots (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  agent_name TEXT NOT NULL,
  input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  cache_read_tokens INTEGER NOT NULL DEFAULT 0,
  cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
  estimated_cost_usd REAL,
  model_used TEXT,
  run_id TEXT,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f','now'))
);
CREATE INDEX IF NOT EXISTS idx_snapshots_agent_time ON token_snapshots(agent_name, created_at);
";

fn row_to_session_metrics(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionMetrics> {
    Ok(SessionMetrics {
        agent_name: row.get(0)?,
        task_id: row.get(1)?,
        capability: row.get(2)?,
        started_at: row.get(3)?,
        completed_at: row.get(4)?,
        duration_ms: row.get(5)?,
        exit_code: row.get(6)?,
        merge_result: row.get(7)?,
        parent_agent: row.get(8)?,
        input_tokens: row.get(9)?,
        output_tokens: row.get(10)?,
        cache_read_tokens: row.get(11)?,
        cache_creation_tokens: row.get(12)?,
        estimated_cost_usd: row.get(13)?,
        model_used: row.get(14)?,
        run_id: row.get(15)?,
    })
}

fn row_to_snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<TokenSnapshot> {
    Ok(TokenSnapshot {
        agent_name: row.get(0)?,
        input_tokens: row.get(1)?,
        output_tokens: row.get(2)?,
        cache_read_tokens: row.get(3)?,
        cache_creation_tokens: row.get(4)?,
        estimated_cost_usd: row.get(5)?,
        model_used: row.get(6)?,
        run_id: row.get(7)?,
        created_at: row.get(8)?,
    })
}

pub struct MetricsStore {
    conn: Connection,
}

impl MetricsStore {
    pub fn new(path: &str) -> Result<Self> {
        let conn = open_connection(path)?;
        conn.execute_batch(METRICS_SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn record_session(&self, metrics: &SessionMetrics) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sessions (agent_name, task_id, capability, started_at, completed_at,
              duration_ms, exit_code, merge_result, parent_agent, input_tokens, output_tokens,
              cache_read_tokens, cache_creation_tokens, estimated_cost_usd, model_used, run_id)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)
             ON CONFLICT(agent_name, task_id) DO UPDATE SET
               capability = excluded.capability,
               started_at = excluded.started_at,
               completed_at = excluded.completed_at,
               duration_ms = excluded.duration_ms,
               exit_code = excluded.exit_code,
               merge_result = excluded.merge_result,
               parent_agent = excluded.parent_agent,
               input_tokens = excluded.input_tokens,
               output_tokens = excluded.output_tokens,
               cache_read_tokens = excluded.cache_read_tokens,
               cache_creation_tokens = excluded.cache_creation_tokens,
               estimated_cost_usd = excluded.estimated_cost_usd,
               model_used = excluded.model_used,
               run_id = excluded.run_id",
            params![
                metrics.agent_name,
                metrics.task_id,
                metrics.capability,
                metrics.started_at,
                metrics.completed_at,
                metrics.duration_ms,
                metrics.exit_code,
                metrics.merge_result,
                metrics.parent_agent,
                metrics.input_tokens,
                metrics.output_tokens,
                metrics.cache_read_tokens,
                metrics.cache_creation_tokens,
                metrics.estimated_cost_usd,
                metrics.model_used,
                metrics.run_id,
            ],
        )?;
        Ok(())
    }

    pub fn get_recent_sessions(&self, limit: Option<i64>) -> Result<Vec<SessionMetrics>> {
        let sql = format!(
            "SELECT agent_name, task_id, capability, started_at, completed_at,
                    duration_ms, exit_code, merge_result, parent_agent, input_tokens,
                    output_tokens, cache_read_tokens, cache_creation_tokens,
                    estimated_cost_usd, model_used, run_id
             FROM sessions ORDER BY started_at DESC{}",
            limit.map(|l| format!(" LIMIT {}", l)).unwrap_or_default()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_session_metrics)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(GroveError::from)
    }

    pub fn get_sessions_by_agent(&self, agent_name: &str) -> Result<Vec<SessionMetrics>> {
        let mut stmt = self.conn.prepare(
            "SELECT agent_name, task_id, capability, started_at, completed_at,
                    duration_ms, exit_code, merge_result, parent_agent, input_tokens,
                    output_tokens, cache_read_tokens, cache_creation_tokens,
                    estimated_cost_usd, model_used, run_id
             FROM sessions WHERE agent_name = ?1 ORDER BY started_at DESC",
        )?;
        let rows = stmt.query_map(params![agent_name], row_to_session_metrics)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(GroveError::from)
    }

    pub fn get_sessions_by_run(&self, run_id: &str) -> Result<Vec<SessionMetrics>> {
        let mut stmt = self.conn.prepare(
            "SELECT agent_name, task_id, capability, started_at, completed_at,
                    duration_ms, exit_code, merge_result, parent_agent, input_tokens,
                    output_tokens, cache_read_tokens, cache_creation_tokens,
                    estimated_cost_usd, model_used, run_id
             FROM sessions WHERE run_id = ?1 ORDER BY started_at DESC",
        )?;
        let rows = stmt.query_map(params![run_id], row_to_session_metrics)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(GroveError::from)
    }

    pub fn get_sessions_by_task(&self, task_id: &str) -> Result<Vec<SessionMetrics>> {
        let mut stmt = self.conn.prepare(
            "SELECT agent_name, task_id, capability, started_at, completed_at,
                    duration_ms, exit_code, merge_result, parent_agent, input_tokens,
                    output_tokens, cache_read_tokens, cache_creation_tokens,
                    estimated_cost_usd, model_used, run_id
             FROM sessions WHERE task_id = ?1 ORDER BY started_at DESC",
        )?;
        let rows = stmt.query_map(params![task_id], row_to_session_metrics)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(GroveError::from)
    }

    pub fn get_average_duration(&self, capability: Option<&str>) -> Result<f64> {
        let avg: f64 = if let Some(cap) = capability {
            self.conn.query_row(
                "SELECT COALESCE(AVG(duration_ms), 0.0) FROM sessions WHERE capability = ?1",
                params![cap],
                |r| r.get(0),
            )?
        } else {
            self.conn.query_row(
                "SELECT COALESCE(AVG(duration_ms), 0.0) FROM sessions",
                [],
                |r| r.get(0),
            )?
        };
        Ok(avg)
    }

    pub fn count_sessions(&self) -> Result<i64> {
        let n = self
            .conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
        Ok(n)
    }

    pub fn purge(&self, opts: PurgeMetricsOpts) -> Result<i64> {
        let n = if opts.all {
            self.conn.execute("DELETE FROM sessions", [])?
        } else if let Some(ref agent) = opts.agent {
            self.conn
                .execute("DELETE FROM sessions WHERE agent_name = ?1", params![agent])?
        } else {
            0
        };
        Ok(n as i64)
    }

    pub fn record_snapshot(&self, snapshot: &TokenSnapshot) -> Result<()> {
        self.conn.execute(
            "INSERT INTO token_snapshots (agent_name, input_tokens, output_tokens,
              cache_read_tokens, cache_creation_tokens, estimated_cost_usd, model_used, run_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                snapshot.agent_name,
                snapshot.input_tokens,
                snapshot.output_tokens,
                snapshot.cache_read_tokens,
                snapshot.cache_creation_tokens,
                snapshot.estimated_cost_usd,
                snapshot.model_used,
                snapshot.run_id,
                snapshot.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_latest_snapshots(&self, run_id: Option<&str>) -> Result<Vec<TokenSnapshot>> {
        let sql = if let Some(rid) = run_id {
            format!(
                "SELECT agent_name, input_tokens, output_tokens, cache_read_tokens,
                        cache_creation_tokens, estimated_cost_usd, model_used, run_id, created_at
                 FROM token_snapshots
                 WHERE id IN (
                   SELECT MAX(id) FROM token_snapshots WHERE run_id = '{}'
                   GROUP BY agent_name
                 )",
                rid.replace('\'', "''")
            )
        } else {
            String::from(
                "SELECT agent_name, input_tokens, output_tokens, cache_read_tokens,
                        cache_creation_tokens, estimated_cost_usd, model_used, run_id, created_at
                 FROM token_snapshots
                 WHERE id IN (
                   SELECT MAX(id) FROM token_snapshots GROUP BY agent_name
                 )",
            )
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_snapshot)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(GroveError::from)
    }

    pub fn get_latest_snapshot_time(&self, agent_name: &str) -> Result<Option<String>> {
        let result = self
            .conn
            .query_row(
                "SELECT MAX(created_at) FROM token_snapshots WHERE agent_name = ?1",
                params![agent_name],
                |r| r.get(0),
            )
            .optional()?;
        Ok(result.flatten())
    }

    pub fn purge_snapshots(&self, opts: PurgeSnapshotOpts) -> Result<i64> {
        let n = if opts.all {
            self.conn.execute("DELETE FROM token_snapshots", [])?
        } else if let Some(ms) = opts.older_than_ms {
            let cutoff_secs = ms / 1000;
            self.conn.execute(
                &format!(
                    "DELETE FROM token_snapshots WHERE created_at < datetime('now', '-{} seconds')",
                    cutoff_secs
                ),
                [],
            )?
        } else if let Some(ref agent) = opts.agent {
            self.conn.execute(
                "DELETE FROM token_snapshots WHERE agent_name = ?1",
                params![agent],
            )?
        } else {
            0
        };
        Ok(n as i64)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_metrics(agent: &str, task: &str) -> SessionMetrics {
        SessionMetrics {
            agent_name: agent.to_string(),
            task_id: task.to_string(),
            capability: "builder".to_string(),
            started_at: "2024-01-01T00:00:00Z".to_string(),
            completed_at: None,
            duration_ms: 1000,
            exit_code: Some(0),
            merge_result: None,
            parent_agent: None,
            input_tokens: 100,
            output_tokens: 200,
            cache_read_tokens: 50,
            cache_creation_tokens: 10,
            estimated_cost_usd: Some(0.01),
            model_used: Some("claude-3".to_string()),
            run_id: Some("run-1".to_string()),
        }
    }

    fn make_snapshot(agent: &str) -> TokenSnapshot {
        TokenSnapshot {
            agent_name: agent.to_string(),
            input_tokens: 100,
            output_tokens: 200,
            cache_read_tokens: 50,
            cache_creation_tokens: 10,
            estimated_cost_usd: Some(0.01),
            model_used: Some("claude-3".to_string()),
            run_id: Some("run-1".to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn test_schema_idempotent() {
        MetricsStore::new(":memory:").unwrap();
    }

    #[test]
    fn test_record_and_get() {
        let store = MetricsStore::new(":memory:").unwrap();
        store
            .record_session(&make_metrics("agent-a", "task-1"))
            .unwrap();
        let sessions = store.get_sessions_by_agent("agent-a").unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].task_id, "task-1");
    }

    #[test]
    fn test_record_upsert() {
        let store = MetricsStore::new(":memory:").unwrap();
        store
            .record_session(&make_metrics("agent-a", "task-1"))
            .unwrap();
        let mut updated = make_metrics("agent-a", "task-1");
        updated.duration_ms = 9999;
        store.record_session(&updated).unwrap();
        let sessions = store.get_sessions_by_agent("agent-a").unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].duration_ms, 9999);
    }

    #[test]
    fn test_get_empty_agent() {
        let store = MetricsStore::new(":memory:").unwrap();
        let result = store.get_sessions_by_agent("nobody").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_count_sessions() {
        let store = MetricsStore::new(":memory:").unwrap();
        assert_eq!(store.count_sessions().unwrap(), 0);
        store.record_session(&make_metrics("a", "t1")).unwrap();
        store.record_session(&make_metrics("a", "t2")).unwrap();
        assert_eq!(store.count_sessions().unwrap(), 2);
    }

    #[test]
    fn test_get_average_duration() {
        let store = MetricsStore::new(":memory:").unwrap();
        let mut m1 = make_metrics("a", "t1");
        m1.duration_ms = 1000;
        let mut m2 = make_metrics("a", "t2");
        m2.duration_ms = 3000;
        store.record_session(&m1).unwrap();
        store.record_session(&m2).unwrap();
        let avg = store.get_average_duration(None).unwrap();
        assert_eq!(avg, 2000.0);
    }

    #[test]
    fn test_get_sessions_by_run() {
        let store = MetricsStore::new(":memory:").unwrap();
        store.record_session(&make_metrics("a", "t1")).unwrap();
        let result = store.get_sessions_by_run("run-1").unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_get_sessions_by_task() {
        let store = MetricsStore::new(":memory:").unwrap();
        store.record_session(&make_metrics("a", "task-1")).unwrap();
        store.record_session(&make_metrics("b", "task-1")).unwrap();
        let result = store.get_sessions_by_task("task-1").unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_purge_all() {
        let store = MetricsStore::new(":memory:").unwrap();
        store.record_session(&make_metrics("a", "t1")).unwrap();
        let deleted = store
            .purge(PurgeMetricsOpts {
                all: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(store.count_sessions().unwrap(), 0);
    }

    #[test]
    fn test_record_snapshot_and_get_latest() {
        let store = MetricsStore::new(":memory:").unwrap();
        store.record_snapshot(&make_snapshot("agent-a")).unwrap();
        store.record_snapshot(&make_snapshot("agent-a")).unwrap();
        store.record_snapshot(&make_snapshot("agent-b")).unwrap();

        let latest = store.get_latest_snapshots(None).unwrap();
        assert_eq!(latest.len(), 2); // one per agent
    }

    #[test]
    fn test_get_latest_snapshot_time() {
        let store = MetricsStore::new(":memory:").unwrap();
        assert!(store.get_latest_snapshot_time("nobody").unwrap().is_none());
        store.record_snapshot(&make_snapshot("agent-a")).unwrap();
        let t = store.get_latest_snapshot_time("agent-a").unwrap();
        assert!(t.is_some());
    }

    #[test]
    fn test_purge_snapshots() {
        let store = MetricsStore::new(":memory:").unwrap();
        store.record_snapshot(&make_snapshot("agent-a")).unwrap();
        let deleted = store
            .purge_snapshots(PurgeSnapshotOpts {
                all: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(deleted, 1);
    }
}
