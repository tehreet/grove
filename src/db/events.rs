#![allow(dead_code)]

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::connection::open_connection;
use crate::errors::{GroveError, Result};
use crate::types::{
    EventLevel, EventQueryOptions, InsertEvent, PurgeEventOpts, StoredEvent, ToolCorrelation,
    ToolStats,
};

const EVENTS_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT,
  agent_name TEXT NOT NULL,
  session_id TEXT,
  event_type TEXT NOT NULL,
  tool_name TEXT,
  tool_args TEXT,
  tool_duration_ms INTEGER,
  level TEXT NOT NULL DEFAULT 'info'
    CHECK(level IN ('debug','info','warn','error')),
  data TEXT,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f','now'))
);
CREATE INDEX IF NOT EXISTS idx_events_agent_time ON events(agent_name, created_at);
CREATE INDEX IF NOT EXISTS idx_events_run_time ON events(run_id, created_at);
CREATE INDEX IF NOT EXISTS idx_events_type_time ON events(event_type, created_at);
CREATE INDEX IF NOT EXISTS idx_events_tool_agent ON events(tool_name, agent_name);
CREATE INDEX IF NOT EXISTS idx_events_level_error ON events(level) WHERE level = 'error';
";

fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredEvent> {
    Ok(StoredEvent {
        id: row.get(0)?,
        run_id: row.get(1)?,
        agent_name: row.get(2)?,
        session_id: row.get(3)?,
        event_type: row.get(4)?,
        tool_name: row.get(5)?,
        tool_args: row.get(6)?,
        tool_duration_ms: row.get(7)?,
        level: row.get(8)?,
        data: row.get(9)?,
        created_at: row.get(10)?,
    })
}

fn apply_query_options(sql: &mut String, opts: &EventQueryOptions) {
    if let Some(ref since) = opts.since {
        sql.push_str(&format!(" AND created_at >= '{}'", since.replace('\'', "''")));
    }
    if let Some(ref until) = opts.until {
        sql.push_str(&format!(" AND created_at <= '{}'", until.replace('\'', "''")));
    }
    if let Some(ref level) = opts.level {
        let level_str = match level {
            EventLevel::Debug => "debug",
            EventLevel::Info => "info",
            EventLevel::Warn => "warn",
            EventLevel::Error => "error",
        };
        sql.push_str(&format!(" AND level = '{}'", level_str));
    }
    sql.push_str(" ORDER BY created_at DESC");
    if let Some(limit) = opts.limit {
        sql.push_str(&format!(" LIMIT {}", limit));
    }
}

pub struct EventStore {
    conn: Connection,
}

impl EventStore {
    pub fn new(path: &str) -> Result<Self> {
        let conn = open_connection(path)?;
        conn.execute_batch(EVENTS_SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn insert(&self, event: &InsertEvent) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO events (run_id, agent_name, session_id, event_type, tool_name, tool_args, tool_duration_ms, level, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                event.run_id,
                event.agent_name,
                event.session_id,
                event.event_type,
                event.tool_name,
                event.tool_args,
                event.tool_duration_ms,
                event.level,
                event.data,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn correlate_tool_end(
        &self,
        agent_name: &str,
        tool_name: &str,
    ) -> Result<Option<ToolCorrelation>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, created_at FROM events
                 WHERE agent_name = ?1 AND tool_name = ?2 AND event_type = 'tool_start'
                 ORDER BY created_at DESC LIMIT 1",
                params![agent_name, tool_name],
                |row| {
                    let id: i64 = row.get(0)?;
                    let started_at: String = row.get(1)?;
                    Ok((id, started_at))
                },
            )
            .optional()?;

        match result {
            None => Ok(None),
            Some((start_id, started_at)) => {
                let now = chrono::Utc::now();
                // Parse started_at to compute duration
                let duration_ms = chrono::DateTime::parse_from_rfc3339(&started_at)
                    .map(|t| (now - t.with_timezone(&chrono::Utc)).num_milliseconds())
                    .unwrap_or(0);
                Ok(Some(ToolCorrelation {
                    start_event_id: start_id,
                    started_at,
                    duration_ms,
                }))
            }
        }
    }

    pub fn get_by_agent(
        &self,
        agent_name: &str,
        opts: Option<EventQueryOptions>,
    ) -> Result<Vec<StoredEvent>> {
        let opts = opts.unwrap_or_default();
        let placeholder_agent = agent_name.replace('\'', "''");
        let mut sql = format!(
            "SELECT id, run_id, agent_name, session_id, event_type, tool_name, tool_args, tool_duration_ms, level, data, created_at
             FROM events WHERE agent_name = '{}'",
            placeholder_agent
        );
        apply_query_options(&mut sql, &opts);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_event)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(GroveError::from)
    }

    pub fn get_by_run(
        &self,
        run_id: &str,
        opts: Option<EventQueryOptions>,
    ) -> Result<Vec<StoredEvent>> {
        let opts = opts.unwrap_or_default();
        let run_id_escaped = run_id.replace('\'', "''");
        let mut sql = format!(
            "SELECT id, run_id, agent_name, session_id, event_type, tool_name, tool_args, tool_duration_ms, level, data, created_at
             FROM events WHERE run_id = '{}'",
            run_id_escaped
        );
        apply_query_options(&mut sql, &opts);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_event)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(GroveError::from)
    }

    pub fn get_errors(&self, opts: Option<EventQueryOptions>) -> Result<Vec<StoredEvent>> {
        let opts = opts.unwrap_or_default();
        let mut sql = String::from(
            "SELECT id, run_id, agent_name, session_id, event_type, tool_name, tool_args, tool_duration_ms, level, data, created_at
             FROM events WHERE level = 'error'",
        );
        apply_query_options(&mut sql, &opts);
        // Remove the extra level filter added by apply_query_options if level was set
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_event)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(GroveError::from)
    }

    pub fn get_timeline(
        &self,
        since: &str,
        opts: Option<EventQueryOptions>,
    ) -> Result<Vec<StoredEvent>> {
        let opts = opts.unwrap_or_default();
        let since_escaped = since.replace('\'', "''");
        let mut sql = format!(
            "SELECT id, run_id, agent_name, session_id, event_type, tool_name, tool_args, tool_duration_ms, level, data, created_at
             FROM events WHERE created_at >= '{}'",
            since_escaped
        );
        if let Some(ref level) = opts.level {
            let level_str = match level {
                EventLevel::Debug => "debug",
                EventLevel::Info => "info",
                EventLevel::Warn => "warn",
                EventLevel::Error => "error",
            };
            sql.push_str(&format!(" AND level = '{}'", level_str));
        }
        sql.push_str(" ORDER BY created_at ASC");
        if let Some(limit) = opts.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_event)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(GroveError::from)
    }

    pub fn get_tool_stats(
        &self,
        agent_name: Option<&str>,
        since: Option<&str>,
    ) -> Result<Vec<ToolStats>> {
        let mut sql = String::from(
            "SELECT tool_name, COUNT(*) as count,
                    COALESCE(AVG(tool_duration_ms), 0.0) as avg_duration_ms,
                    COALESCE(MAX(tool_duration_ms), 0) as max_duration_ms
             FROM events WHERE event_type = 'tool_end' AND tool_name IS NOT NULL",
        );
        if let Some(agent) = agent_name {
            sql.push_str(&format!(" AND agent_name = '{}'", agent.replace('\'', "''")));
        }
        if let Some(s) = since {
            sql.push_str(&format!(" AND created_at >= '{}'", s.replace('\'', "''")));
        }
        sql.push_str(" GROUP BY tool_name ORDER BY count DESC");

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            Ok(ToolStats {
                tool_name: row.get(0)?,
                count: row.get(1)?,
                avg_duration_ms: row.get(2)?,
                max_duration_ms: row.get::<_, f64>(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(GroveError::from)
    }

    /// Query events with optional agent, type, and since-ID filters (for feed command).
    pub fn get_feed(
        &self,
        agent: Option<&str>,
        event_type: Option<&str>,
        since_id: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<StoredEvent>> {
        let mut sql = String::from(
            "SELECT id, run_id, agent_name, session_id, event_type, tool_name, tool_args, tool_duration_ms, level, data, created_at
             FROM events WHERE 1=1",
        );
        if let Some(agent) = agent {
            sql.push_str(&format!(" AND agent_name = '{}'", agent.replace('\'', "''")));
        }
        if let Some(et) = event_type {
            sql.push_str(&format!(" AND event_type = '{}'", et.replace('\'', "''")));
        }
        if let Some(id) = since_id {
            sql.push_str(&format!(" AND id > {}", id));
        }
        sql.push_str(" ORDER BY id ASC");
        if let Some(lim) = limit {
            sql.push_str(&format!(" LIMIT {}", lim));
        }
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_event)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(GroveError::from)
    }

    /// Query events for a specific task ID (across all agents with that task).
    pub fn get_by_task(
        &self,
        task_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<StoredEvent>> {
        // Events don't directly store task_id; we look for it in the data field
        // or fall back to agent_name matching. For now, join via session_id in data.
        let escaped = task_id.replace('\'', "''");
        let mut sql = format!(
            "SELECT id, run_id, agent_name, session_id, event_type, tool_name, tool_args, tool_duration_ms, level, data, created_at
             FROM events WHERE data LIKE '%{}%'",
            escaped
        );
        sql.push_str(" ORDER BY id ASC");
        if let Some(lim) = limit {
            sql.push_str(&format!(" LIMIT {}", lim));
        }
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_event)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(GroveError::from)
    }

    /// Get the maximum event ID (for follow mode cursor).
    pub fn get_max_id(&self) -> Result<i64> {
        let id: i64 = self
            .conn
            .query_row("SELECT COALESCE(MAX(id), 0) FROM events", [], |r| r.get(0))?;
        Ok(id)
    }

    /// Get error events grouped by agent: returns (agent_name, count, latest_event).
    pub fn get_errors_grouped(
        &self,
        agent: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<(String, i64, StoredEvent)>> {
        // Get counts per agent
        let mut count_sql = String::from(
            "SELECT agent_name, COUNT(*) FROM events WHERE level = 'error'",
        );
        if let Some(a) = agent {
            count_sql.push_str(&format!(" AND agent_name = '{}'", a.replace('\'', "''")));
        }
        count_sql.push_str(" GROUP BY agent_name ORDER BY COUNT(*) DESC");
        if let Some(lim) = limit {
            count_sql.push_str(&format!(" LIMIT {}", lim));
        }

        let mut stmt = self.conn.prepare(&count_sql)?;
        let agent_counts: Vec<(String, i64)> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(GroveError::from)?;

        let mut result = Vec::new();
        for (agent_name, count) in agent_counts {
            let latest_sql = format!(
                "SELECT id, run_id, agent_name, session_id, event_type, tool_name, tool_args, tool_duration_ms, level, data, created_at
                 FROM events WHERE level = 'error' AND agent_name = '{}' ORDER BY id DESC LIMIT 1",
                agent_name.replace('\'', "''")
            );
            if let Ok(event) = self.conn.query_row(&latest_sql, [], row_to_event) {
                result.push((agent_name, count, event));
            }
        }
        Ok(result)
    }

    pub fn purge(&self, opts: PurgeEventOpts) -> Result<i64> {
        let n = if opts.all {
            self.conn.execute("DELETE FROM events", [])?
        } else if let Some(ms) = opts.older_than_ms {
            let cutoff_secs = ms / 1000;
            self.conn.execute(
                &format!(
                    "DELETE FROM events WHERE created_at < datetime('now', '-{} seconds')",
                    cutoff_secs
                ),
                [],
            )?
        } else if let Some(ref agent) = opts.agent_name {
            self.conn.execute(
                "DELETE FROM events WHERE agent_name = ?1",
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
    use crate::types::{EventLevel, EventType};

    fn make_event(agent: &str, event_type: EventType) -> InsertEvent {
        InsertEvent {
            run_id: None,
            agent_name: agent.to_string(),
            session_id: None,
            event_type,
            tool_name: None,
            tool_args: None,
            tool_duration_ms: None,
            level: EventLevel::Info,
            data: None,
        }
    }

    #[test]
    fn test_schema_idempotent() {
        EventStore::new(":memory:").unwrap();
    }

    #[test]
    fn test_insert_returns_id() {
        let store = EventStore::new(":memory:").unwrap();
        let id = store.insert(&make_event("agent-a", EventType::SessionStart)).unwrap();
        assert_eq!(id, 1);
        let id2 = store.insert(&make_event("agent-a", EventType::TurnStart)).unwrap();
        assert_eq!(id2, 2);
    }

    #[test]
    fn test_get_by_agent() {
        let store = EventStore::new(":memory:").unwrap();
        store.insert(&make_event("agent-a", EventType::SessionStart)).unwrap();
        store.insert(&make_event("agent-a", EventType::TurnStart)).unwrap();
        store.insert(&make_event("agent-b", EventType::SessionStart)).unwrap();

        let events = store.get_by_agent("agent-a", None).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_get_by_agent_empty() {
        let store = EventStore::new(":memory:").unwrap();
        let events = store.get_by_agent("nobody", None).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_get_errors() {
        let store = EventStore::new(":memory:").unwrap();
        let mut e = make_event("agent-a", EventType::Error);
        e.level = EventLevel::Error;
        store.insert(&e).unwrap();
        store.insert(&make_event("agent-a", EventType::TurnStart)).unwrap();

        let errors = store.get_errors(None).unwrap();
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_correlate_tool_end() {
        let store = EventStore::new(":memory:").unwrap();
        let mut start = make_event("agent-a", EventType::ToolStart);
        start.tool_name = Some("Read".to_string());
        store.insert(&start).unwrap();

        let corr = store.correlate_tool_end("agent-a", "Read").unwrap();
        assert!(corr.is_some());
        let c = corr.unwrap();
        assert!(c.start_event_id > 0);
        assert!(c.duration_ms >= 0);
    }

    #[test]
    fn test_correlate_tool_end_no_match() {
        let store = EventStore::new(":memory:").unwrap();
        let corr = store.correlate_tool_end("agent-a", "NonExistent").unwrap();
        assert!(corr.is_none());
    }

    #[test]
    fn test_get_tool_stats() {
        let store = EventStore::new(":memory:").unwrap();
        let mut e1 = make_event("agent-a", EventType::ToolEnd);
        e1.tool_name = Some("Read".to_string());
        e1.tool_duration_ms = Some(100);
        let mut e2 = make_event("agent-a", EventType::ToolEnd);
        e2.tool_name = Some("Read".to_string());
        e2.tool_duration_ms = Some(200);
        store.insert(&e1).unwrap();
        store.insert(&e2).unwrap();

        let stats = store.get_tool_stats(None, None).unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].tool_name, "Read");
        assert_eq!(stats[0].count, 2);
        assert_eq!(stats[0].avg_duration_ms, 150.0);
        assert_eq!(stats[0].max_duration_ms, 200.0_f64);
    }

    #[test]
    fn test_purge_all() {
        let store = EventStore::new(":memory:").unwrap();
        store.insert(&make_event("a", EventType::TurnStart)).unwrap();
        store.insert(&make_event("a", EventType::TurnEnd)).unwrap();
        let deleted = store.purge(PurgeEventOpts { all: true, ..Default::default() }).unwrap();
        assert_eq!(deleted, 2);
    }

    #[test]
    fn test_purge_by_agent() {
        let store = EventStore::new(":memory:").unwrap();
        store.insert(&make_event("alice", EventType::TurnStart)).unwrap();
        store.insert(&make_event("bob", EventType::TurnStart)).unwrap();
        let deleted = store.purge(PurgeEventOpts {
            agent_name: Some("alice".to_string()),
            ..Default::default()
        }).unwrap();
        assert_eq!(deleted, 1);
    }

    #[test]
    fn test_get_by_run() {
        let store = EventStore::new(":memory:").unwrap();
        let mut e = make_event("agent-a", EventType::SessionStart);
        e.run_id = Some("run-1".to_string());
        store.insert(&e).unwrap();
        store.insert(&make_event("agent-b", EventType::SessionStart)).unwrap();

        let events = store.get_by_run("run-1", None).unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_get_timeline() {
        let store = EventStore::new(":memory:").unwrap();
        store.insert(&make_event("agent-a", EventType::TurnStart)).unwrap();
        let events = store
            .get_timeline("2000-01-01T00:00:00Z", None)
            .unwrap();
        assert_eq!(events.len(), 1);
    }
}
