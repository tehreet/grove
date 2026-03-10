#![allow(dead_code)]

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::connection::open_connection;
use crate::errors::{GroveError, Result};
use crate::types::{InsertMergeEntry, MergeEntry, MergeEntryStatus, ResolutionTier};

const MERGE_QUEUE_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS merge_queue (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  branch_name TEXT NOT NULL,
  task_id TEXT NOT NULL,
  agent_name TEXT NOT NULL,
  files_modified TEXT NOT NULL DEFAULT '[]',
  enqueued_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f','now')),
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK(status IN ('pending','merging','merged','conflict','failed')),
  resolved_tier TEXT
    CHECK(resolved_tier IS NULL OR resolved_tier IN ('clean-merge','auto-resolve','ai-resolve','reimagine'))
);
CREATE INDEX IF NOT EXISTS idx_merge_queue_status ON merge_queue(status);
CREATE INDEX IF NOT EXISTS idx_merge_queue_branch ON merge_queue(branch_name);
";

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<MergeEntry> {
    let files_json: String = row.get(4)?;
    let files_modified: Vec<String> = serde_json::from_str(&files_json).unwrap_or_default();
    Ok(MergeEntry {
        id: row.get(0)?,
        branch_name: row.get(1)?,
        task_id: row.get(2)?,
        agent_name: row.get(3)?,
        files_modified,
        enqueued_at: row.get(5)?,
        status: row.get(6)?,
        resolved_tier: row.get(7)?,
    })
}

pub struct MergeQueue {
    conn: Connection,
}

impl MergeQueue {
    pub fn new(path: &str) -> Result<Self> {
        let conn = open_connection(path)?;
        conn.execute_batch(MERGE_QUEUE_SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn enqueue(&self, entry: &InsertMergeEntry) -> Result<MergeEntry> {
        let files_json = serde_json::to_string(&entry.files_modified)?;
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO merge_queue (branch_name, task_id, agent_name, files_modified, enqueued_at, status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending')",
            params![entry.branch_name, entry.task_id, entry.agent_name, files_json, now],
        )?;
        let id = self.conn.last_insert_rowid();
        Ok(MergeEntry {
            id,
            branch_name: entry.branch_name.clone(),
            task_id: entry.task_id.clone(),
            agent_name: entry.agent_name.clone(),
            files_modified: entry.files_modified.clone(),
            enqueued_at: now,
            status: MergeEntryStatus::Pending,
            resolved_tier: None,
        })
    }

    pub fn dequeue(&mut self) -> Result<Option<MergeEntry>> {
        let tx = self.conn.transaction()?;
        let result = tx
            .query_row(
                "SELECT id, branch_name, task_id, agent_name, files_modified, enqueued_at, status, resolved_tier
                 FROM merge_queue WHERE status = 'pending' ORDER BY id ASC LIMIT 1",
                [],
                row_to_entry,
            )
            .optional()?;

        if let Some(ref entry) = result {
            tx.execute(
                "UPDATE merge_queue SET status = 'merging' WHERE id = ?1",
                params![entry.id],
            )?;
        }
        tx.commit()?;

        // Return with updated status if we found an entry
        Ok(result.map(|mut e| {
            e.status = MergeEntryStatus::Merging;
            e
        }))
    }

    pub fn peek(&self) -> Result<Option<MergeEntry>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, branch_name, task_id, agent_name, files_modified, enqueued_at, status, resolved_tier
                 FROM merge_queue WHERE status = 'pending' ORDER BY id ASC LIMIT 1",
                [],
                row_to_entry,
            )
            .optional()?;
        Ok(result)
    }

    pub fn list(&self, status: Option<MergeEntryStatus>) -> Result<Vec<MergeEntry>> {
        let sql = if let Some(ref s) = status {
            let s_str = match s {
                MergeEntryStatus::Pending => "pending",
                MergeEntryStatus::Merging => "merging",
                MergeEntryStatus::Merged => "merged",
                MergeEntryStatus::Conflict => "conflict",
                MergeEntryStatus::Failed => "failed",
            };
            format!(
                "SELECT id, branch_name, task_id, agent_name, files_modified, enqueued_at, status, resolved_tier
                 FROM merge_queue WHERE status = '{}' ORDER BY id ASC",
                s_str
            )
        } else {
            String::from(
                "SELECT id, branch_name, task_id, agent_name, files_modified, enqueued_at, status, resolved_tier
                 FROM merge_queue ORDER BY id ASC",
            )
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_entry)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(GroveError::from)
    }

    pub fn update_status(
        &self,
        branch_name: &str,
        status: MergeEntryStatus,
        tier: Option<ResolutionTier>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE merge_queue SET status = ?1, resolved_tier = ?2 WHERE branch_name = ?3",
            params![status, tier, branch_name],
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

    fn make_entry(branch: &str) -> InsertMergeEntry {
        InsertMergeEntry {
            branch_name: branch.to_string(),
            task_id: "task-001".to_string(),
            agent_name: "agent-a".to_string(),
            files_modified: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
        }
    }

    #[test]
    fn test_schema_idempotent() {
        MergeQueue::new(":memory:").unwrap();
    }

    #[test]
    fn test_enqueue_and_list() {
        let store = MergeQueue::new(":memory:").unwrap();
        let entry = store.enqueue(&make_entry("feature-a")).unwrap();
        assert_eq!(entry.branch_name, "feature-a");
        assert_eq!(entry.status, MergeEntryStatus::Pending);
        assert_eq!(entry.files_modified, vec!["src/main.rs", "src/lib.rs"]);

        let list = store.list(None).unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_files_modified_roundtrip() {
        let store = MergeQueue::new(":memory:").unwrap();
        let entry = store.enqueue(&make_entry("branch-x")).unwrap();
        let list = store.list(None).unwrap();
        assert_eq!(list[0].files_modified, entry.files_modified);
    }

    #[test]
    fn test_peek() {
        let store = MergeQueue::new(":memory:").unwrap();
        assert!(store.peek().unwrap().is_none());
        store.enqueue(&make_entry("branch-a")).unwrap();
        let peeked = store.peek().unwrap().unwrap();
        assert_eq!(peeked.branch_name, "branch-a");
        // Peek doesn't change status
        let peeked2 = store.peek().unwrap().unwrap();
        assert_eq!(peeked2.status, MergeEntryStatus::Pending);
    }

    #[test]
    fn test_dequeue_fifo_order() {
        let mut store = MergeQueue::new(":memory:").unwrap();
        store.enqueue(&make_entry("branch-a")).unwrap();
        store.enqueue(&make_entry("branch-b")).unwrap();

        let first = store.dequeue().unwrap().unwrap();
        assert_eq!(first.branch_name, "branch-a");
        assert_eq!(first.status, MergeEntryStatus::Merging);

        let second = store.dequeue().unwrap().unwrap();
        assert_eq!(second.branch_name, "branch-b");
    }

    #[test]
    fn test_dequeue_empty() {
        let mut store = MergeQueue::new(":memory:").unwrap();
        assert!(store.dequeue().unwrap().is_none());
    }

    #[test]
    fn test_update_status() {
        let store = MergeQueue::new(":memory:").unwrap();
        store.enqueue(&make_entry("branch-a")).unwrap();
        store
            .update_status(
                "branch-a",
                MergeEntryStatus::Merged,
                Some(ResolutionTier::CleanMerge),
            )
            .unwrap();

        let list = store.list(Some(MergeEntryStatus::Merged)).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].resolved_tier, Some(ResolutionTier::CleanMerge));
    }

    #[test]
    fn test_list_by_status() {
        let store = MergeQueue::new(":memory:").unwrap();
        store.enqueue(&make_entry("a")).unwrap();
        store.enqueue(&make_entry("b")).unwrap();
        store
            .update_status("a", MergeEntryStatus::Merged, None)
            .unwrap();

        let pending = store.list(Some(MergeEntryStatus::Pending)).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].branch_name, "b");

        let merged = store.list(Some(MergeEntryStatus::Merged)).unwrap();
        assert_eq!(merged.len(), 1);
    }
}
