#![allow(dead_code)]

use rand::distributions::Alphanumeric;
use rand::Rng;
use rusqlite::{params, Connection, OptionalExtension};

use crate::db::connection::open_connection;
use crate::errors::{GroveError, Result};
use crate::types::{InsertMailMessage, MailFilters, MailMessage, PurgeMailOpts};

const MAIL_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS messages (
  id TEXT PRIMARY KEY,
  from_agent TEXT NOT NULL,
  to_agent TEXT NOT NULL,
  subject TEXT NOT NULL,
  body TEXT NOT NULL,
  type TEXT NOT NULL DEFAULT 'status'
    CHECK(type IN ('status','question','result','error','worker_done','merge_ready','merged','merge_failed','escalation','health_check','dispatch','assign')),
  priority TEXT NOT NULL DEFAULT 'normal'
    CHECK(priority IN ('low','normal','high','urgent')),
  thread_id TEXT,
  payload TEXT,
  read INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_inbox ON messages(to_agent, read);
CREATE INDEX IF NOT EXISTS idx_thread ON messages(thread_id);
";

fn generate_mail_id() -> String {
    let suffix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(12)
        .map(char::from)
        .collect::<String>()
        .to_lowercase();
    format!("msg-{}", suffix)
}

fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<MailMessage> {
    let read_int: i64 = row.get(9)?;
    Ok(MailMessage {
        id: row.get(0)?,
        from: row.get(1)?,
        to: row.get(2)?,
        subject: row.get(3)?,
        body: row.get(4)?,
        message_type: row.get(5)?,
        priority: row.get(6)?,
        thread_id: row.get(7)?,
        payload: row.get(8)?,
        read: read_int != 0,
        created_at: row.get(10)?,
    })
}

pub struct MailStore {
    conn: Connection,
}

impl MailStore {
    pub fn new(path: &str) -> Result<Self> {
        let conn = open_connection(path)?;
        conn.execute_batch(MAIL_SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn insert(&self, message: &InsertMailMessage) -> Result<MailMessage> {
        let id = message
            .id
            .clone()
            .unwrap_or_else(generate_mail_id);
        let now = chrono::Utc::now().to_rfc3339();

        self.conn.execute(
            "INSERT INTO messages (id, from_agent, to_agent, subject, body, type, priority, thread_id, payload, read, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10)",
            params![
                id,
                message.from_agent,
                message.to_agent,
                message.subject,
                message.body,
                message.message_type,
                message.priority,
                message.thread_id,
                message.payload,
                now,
            ],
        )?;

        Ok(MailMessage {
            id,
            from: message.from_agent.clone(),
            to: message.to_agent.clone(),
            subject: message.subject.clone(),
            body: message.body.clone(),
            message_type: message.message_type,
            priority: message.priority,
            thread_id: message.thread_id.clone(),
            payload: message.payload.clone(),
            read: false,
            created_at: now,
        })
    }

    pub fn get_unread(&self, agent_name: &str) -> Result<Vec<MailMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, from_agent, to_agent, subject, body, type, priority, thread_id, payload, read, created_at
             FROM messages WHERE to_agent = ?1 AND read = 0 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![agent_name], row_to_message)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(GroveError::from)
    }

    pub fn get_all(&self, filters: Option<MailFilters>) -> Result<Vec<MailMessage>> {
        let filters = filters.unwrap_or_default();
        let mut sql = String::from(
            "SELECT id, from_agent, to_agent, subject, body, type, priority, thread_id, payload, read, created_at
             FROM messages WHERE 1=1",
        );
        if let Some(ref from) = filters.from_agent {
            sql.push_str(&format!(" AND from_agent = '{}'", from.replace('\'', "''")));
        }
        if let Some(ref to) = filters.to_agent {
            sql.push_str(&format!(" AND to_agent = '{}'", to.replace('\'', "''")));
        }
        if let Some(unread) = filters.unread {
            sql.push_str(&format!(" AND read = {}", if unread { 0 } else { 1 }));
        }
        sql.push_str(" ORDER BY created_at DESC");
        if let Some(limit) = filters.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_message)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(GroveError::from)
    }

    pub fn get_by_id(&self, id: &str) -> Result<Option<MailMessage>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, from_agent, to_agent, subject, body, type, priority, thread_id, payload, read, created_at
                 FROM messages WHERE id = ?1",
                params![id],
                row_to_message,
            )
            .optional()?;
        Ok(result)
    }

    pub fn get_by_thread(&self, thread_id: &str) -> Result<Vec<MailMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, from_agent, to_agent, subject, body, type, priority, thread_id, payload, read, created_at
             FROM messages WHERE thread_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![thread_id], row_to_message)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(GroveError::from)
    }

    pub fn mark_read(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE messages SET read = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn purge(&self, opts: PurgeMailOpts) -> Result<i64> {
        let n = if opts.all {
            self.conn.execute("DELETE FROM messages", [])?
        } else if let Some(ms) = opts.older_than_ms {
            // Compute cutoff: now - ms
            let cutoff_secs = ms / 1000;
            self.conn.execute(
                &format!(
                    "DELETE FROM messages WHERE created_at < datetime('now', '-{} seconds')",
                    cutoff_secs
                ),
                [],
            )?
        } else if let Some(ref agent) = opts.agent {
            self.conn.execute(
                "DELETE FROM messages WHERE from_agent = ?1 OR to_agent = ?1",
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
    use crate::types::{MailFilters, MailMessageType, MailPriority, PurgeMailOpts};

    fn make_message(from: &str, to: &str) -> InsertMailMessage {
        InsertMailMessage {
            id: None,
            from_agent: from.to_string(),
            to_agent: to.to_string(),
            subject: "test subject".to_string(),
            body: "test body".to_string(),
            priority: MailPriority::Normal,
            message_type: MailMessageType::Status,
            thread_id: None,
            payload: None,
        }
    }

    #[test]
    fn test_schema_idempotent() {
        MailStore::new(":memory:").unwrap();
    }

    #[test]
    fn test_insert_auto_generates_id() {
        let store = MailStore::new(":memory:").unwrap();
        let msg = store.insert(&make_message("a", "b")).unwrap();
        assert!(msg.id.starts_with("msg-"));
        assert_eq!(msg.id.len(), "msg-".len() + 12);
    }

    #[test]
    fn test_insert_with_explicit_id() {
        let store = MailStore::new(":memory:").unwrap();
        let mut m = make_message("a", "b");
        m.id = Some("msg-custom-id".to_string());
        let msg = store.insert(&m).unwrap();
        assert_eq!(msg.id, "msg-custom-id");
    }

    #[test]
    fn test_get_unread() {
        let store = MailStore::new(":memory:").unwrap();
        store.insert(&make_message("a", "bob")).unwrap();
        store.insert(&make_message("a", "bob")).unwrap();
        store.insert(&make_message("a", "alice")).unwrap();

        let unread = store.get_unread("bob").unwrap();
        assert_eq!(unread.len(), 2);
    }

    #[test]
    fn test_mark_read() {
        let store = MailStore::new(":memory:").unwrap();
        let msg = store.insert(&make_message("a", "bob")).unwrap();
        store.mark_read(&msg.id).unwrap();

        let unread = store.get_unread("bob").unwrap();
        assert_eq!(unread.len(), 0);

        let fetched = store.get_by_id(&msg.id).unwrap().unwrap();
        assert!(fetched.read);
    }

    #[test]
    fn test_get_by_id_nonexistent() {
        let store = MailStore::new(":memory:").unwrap();
        assert!(store.get_by_id("nope").unwrap().is_none());
    }

    #[test]
    fn test_get_by_thread() {
        let store = MailStore::new(":memory:").unwrap();
        let mut m1 = make_message("a", "b");
        m1.thread_id = Some("thread-1".to_string());
        let mut m2 = make_message("b", "a");
        m2.thread_id = Some("thread-1".to_string());
        store.insert(&m1).unwrap();
        store.insert(&m2).unwrap();
        store.insert(&make_message("x", "y")).unwrap();

        let thread = store.get_by_thread("thread-1").unwrap();
        assert_eq!(thread.len(), 2);
    }

    #[test]
    fn test_get_all_filters() {
        let store = MailStore::new(":memory:").unwrap();
        store.insert(&make_message("alice", "bob")).unwrap();
        store.insert(&make_message("bob", "alice")).unwrap();

        let to_bob = store.get_all(Some(MailFilters { to_agent: Some("bob".to_string()), ..Default::default() })).unwrap();
        assert_eq!(to_bob.len(), 1);
        assert_eq!(to_bob[0].to, "bob");
    }

    #[test]
    fn test_purge_all() {
        let store = MailStore::new(":memory:").unwrap();
        store.insert(&make_message("a", "b")).unwrap();
        store.insert(&make_message("c", "d")).unwrap();
        let deleted = store.purge(PurgeMailOpts { all: true, ..Default::default() }).unwrap();
        assert_eq!(deleted, 2);
        assert!(store.get_all(None).unwrap().is_empty());
    }

    #[test]
    fn test_purge_by_agent() {
        let store = MailStore::new(":memory:").unwrap();
        store.insert(&make_message("alice", "bob")).unwrap();
        store.insert(&make_message("carol", "dave")).unwrap();
        let deleted = store.purge(PurgeMailOpts { agent: Some("alice".to_string()), ..Default::default() }).unwrap();
        assert_eq!(deleted, 1);
    }
}
