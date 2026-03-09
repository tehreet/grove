//! `grove mail` — mail system subcommands: list, check, read.

use crate::config::resolve_project_root;
use crate::db::mail::MailStore;
use crate::json::{json_error, json_output};
use crate::logging::{format_relative_time, muted, pad_visible, render_header};
use crate::types::{MailFilters, MailMessage};

// ---------------------------------------------------------------------------
// Database path helper
// ---------------------------------------------------------------------------

fn mail_db_path() -> Result<String, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, None).map_err(|e| e.to_string())?;
    Ok(root
        .join(".overstory")
        .join("mail.db")
        .to_string_lossy()
        .to_string())
}

// ---------------------------------------------------------------------------
// execute_list
// ---------------------------------------------------------------------------

pub fn execute_list(
    from: Option<String>,
    to: Option<String>,
    message_type: Option<String>,
    unread: bool,
    limit: Option<i64>,
    json: bool,
) -> Result<(), String> {
    let store = MailStore::new(&mail_db_path()?).map_err(|e| e.to_string())?;
    let filters = MailFilters {
        from_agent: from,
        to_agent: to,
        message_type,
        unread: if unread { Some(true) } else { None },
        limit,
    };
    let messages = store.get_all(Some(filters)).map_err(|e| e.to_string())?;

    if json {
        #[derive(serde::Serialize)]
        struct Output {
            messages: Vec<MailMessage>,
        }
        println!("{}", json_output("mail list", &Output { messages }));
        return Ok(());
    }

    // Text output: table
    println!("{}", render_header("Mail", None));
    if messages.is_empty() {
        println!("  {}", muted("No messages"));
        return Ok(());
    }

    for msg in &messages {
        let read_marker = if msg.read { " " } else { "•" };
        let subject_trunc = truncate_str(&msg.subject, 40);
        let time = format_relative_time(&msg.created_at);
        println!(
            "  {} {}  {}  {}  {} | {}  {}",
            read_marker,
            pad_visible(&msg.from, 20),
            pad_visible(&msg.to, 20),
            pad_visible(&subject_trunc, 42),
            msg.message_type,
            msg.priority,
            muted(&time),
        );
    }
    println!(
        "\n{}",
        muted(&format!(
            "Total: {} message{}",
            messages.len(),
            if messages.len() == 1 { "" } else { "s" }
        ))
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// execute_check
// ---------------------------------------------------------------------------

pub fn execute_check(agent: &str, inject: bool, json: bool) -> Result<(), String> {
    let store = MailStore::new(&mail_db_path()?).map_err(|e| e.to_string())?;
    let messages = store.get_unread(agent).map_err(|e| e.to_string())?;

    if json {
        #[derive(serde::Serialize)]
        struct Output {
            messages: Vec<MailMessage>,
            count: usize,
        }
        let count = messages.len();
        println!("{}", json_output("mail check", &Output { messages, count }));
        return Ok(());
    }

    if inject {
        for msg in &messages {
            println!("──────────────────");
            println!("From: {}", msg.from);
            println!("Subject: {}", msg.subject);
            println!("Type: {} | Priority: {}", msg.message_type, msg.priority);
            println!("Date: {}", msg.created_at);
            println!();
            println!("{}", msg.body);
            println!("──────────────────");
        }
        // Mark as read after inject
        for msg in &messages {
            let _ = store.mark_read(&msg.id);
        }
        return Ok(());
    }

    // Text output
    if messages.is_empty() {
        println!("  {}", muted("No new messages"));
        return Ok(());
    }
    println!(
        "  {} unread message{}\n",
        messages.len(),
        if messages.len() == 1 { "" } else { "s" }
    );
    for msg in &messages {
        let subject_trunc = truncate_str(&msg.subject, 50);
        let time = format_relative_time(&msg.created_at);
        println!(
            "  {}  {}  {}  ({})",
            pad_visible(&msg.from, 20),
            pad_visible(&msg.to, 20),
            subject_trunc,
            muted(&time),
        );
    }
    println!(
        "\n{}",
        muted(&format!(
            "Total: {} message{}",
            messages.len(),
            if messages.len() == 1 { "" } else { "s" }
        ))
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// execute_read
// ---------------------------------------------------------------------------

pub fn execute_read(id: &str, json: bool) -> Result<(), String> {
    let store = MailStore::new(&mail_db_path()?).map_err(|e| e.to_string())?;
    let msg = store.get_by_id(id).map_err(|e| e.to_string())?;

    match msg {
        None => {
            if json {
                println!(
                    "{}",
                    json_error("mail read", &format!("message not found: {id}"))
                );
            } else {
                eprintln!("Message not found: {id}");
            }
            Err(format!("message not found: {id}"))
        }
        Some(msg) => {
            // Mark as read
            let _ = store.mark_read(&msg.id);

            if json {
                println!("{}", json_output("mail read", &msg));
                return Ok(());
            }

            // Text output: full message
            println!("{}", render_header("Mail Message", None));
            println!("  ID:       {}", msg.id);
            println!("  From:     {}", msg.from);
            println!("  To:       {}", msg.to);
            println!("  Subject:  {}", msg.subject);
            println!("  Type:     {}", msg.message_type);
            println!("  Priority: {}", msg.priority);
            if let Some(ref tid) = msg.thread_id {
                println!("  Thread:   {tid}");
            }
            println!("  Date:     {}", msg.created_at);
            println!("  Read:     {}", if msg.read { "yes" } else { "no" });
            println!();
            println!("{}", msg.body);
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn truncate_str(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        let truncated: String = chars[..max - 1].iter().collect();
        format!("{truncated}…")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::mail::MailStore;
    use crate::types::{InsertMailMessage, MailMessageType, MailPriority};

    fn make_msg(from: &str, to: &str, subject: &str) -> InsertMailMessage {
        InsertMailMessage {
            id: None,
            from_agent: from.to_string(),
            to_agent: to.to_string(),
            subject: subject.to_string(),
            body: "test body".to_string(),
            priority: MailPriority::Normal,
            message_type: MailMessageType::Status,
            thread_id: None,
            payload: None,
        }
    }

    #[test]
    fn test_mail_db_path_smoke() {
        // Just ensure the function doesn't panic; result depends on environment
        let _result = mail_db_path();
    }

    #[test]
    fn test_list_via_store() {
        let store = MailStore::new(":memory:").unwrap();
        store.insert(&make_msg("alice", "bob", "hello")).unwrap();
        store.insert(&make_msg("bob", "carol", "world")).unwrap();
        let msgs = store.get_all(None).unwrap();
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn test_check_returns_unread() {
        let store = MailStore::new(":memory:").unwrap();
        let msg = store.insert(&make_msg("alice", "bob", "hello")).unwrap();
        store.insert(&make_msg("carol", "dave", "other")).unwrap();
        let unread = store.get_unread("bob").unwrap();
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].id, msg.id);
    }

    #[test]
    fn test_read_marks_message_read() {
        let store = MailStore::new(":memory:").unwrap();
        let msg = store.insert(&make_msg("alice", "bob", "hello")).unwrap();
        assert!(!msg.read);
        store.mark_read(&msg.id).unwrap();
        let fetched = store.get_by_id(&msg.id).unwrap().unwrap();
        assert!(fetched.read);
        assert!(store.get_unread("bob").unwrap().is_empty());
    }

    #[test]
    fn test_get_by_id_missing() {
        let store = MailStore::new(":memory:").unwrap();
        assert!(store.get_by_id("msg-nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_list_with_from_filter() {
        let store = MailStore::new(":memory:").unwrap();
        store.insert(&make_msg("alice", "bob", "a")).unwrap();
        store.insert(&make_msg("carol", "bob", "b")).unwrap();
        let filters = MailFilters {
            from_agent: Some("alice".to_string()),
            to_agent: None,
            message_type: None,
            unread: None,
            limit: None,
        };
        let msgs = store.get_all(Some(filters)).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].from, "alice");
    }

    #[test]
    fn test_list_with_type_filter() {
        let store = MailStore::new(":memory:").unwrap();
        store.insert(&make_msg("a", "b", "status msg")).unwrap();
        let mut question = make_msg("a", "b", "question msg");
        question.message_type = MailMessageType::Question;
        store.insert(&question).unwrap();

        let filters = MailFilters {
            from_agent: None,
            to_agent: None,
            message_type: Some("question".to_string()),
            unread: None,
            limit: None,
        };
        let msgs = store.get_all(Some(filters)).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].message_type, MailMessageType::Question);
    }

    #[test]
    fn test_truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_str_long() {
        let result = truncate_str("hello world this is long", 10);
        assert_eq!(result.chars().count(), 10);
        assert!(result.ends_with('…'));
    }
}
