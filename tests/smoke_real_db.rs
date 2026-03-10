//! Smoke test: read real .overstory databases that overstory created.
//! Run with: cargo test --test smoke_real_db

use std::path::Path;

// We need to import grove's db modules
// Since grove is a binary crate, we test by using its library-style modules
// For now, let's just verify the databases are readable with rusqlite directly

#[test]
fn read_real_sessions_db() {
    let db_path = Path::new(".overstory/sessions.db");
    if !db_path.exists() {
        eprintln!("Skipping: no sessions.db found");
        return;
    }
    let conn =
        rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("Failed to open sessions.db");

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
        .expect("Failed to query sessions");

    println!("sessions.db: {} sessions", count);
    assert!(count >= 0);

    // Read actual agent names
    let mut stmt = conn
        .prepare(
            "SELECT agent_name, capability, state FROM sessions ORDER BY started_at DESC LIMIT 5",
        )
        .expect("Failed to prepare");
    let rows: Vec<(String, String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .expect("Failed to query")
        .filter_map(|r| r.ok())
        .collect();

    for (name, cap, state) in &rows {
        println!("  {} ({}) -> {}", name, cap, state);
    }
    // After clean, sessions may be empty — just verify the query works
    println!("  {} sessions found", rows.len());
}

#[test]
fn read_real_mail_db() {
    let db_path = Path::new(".overstory/mail.db");
    if !db_path.exists() {
        eprintln!("Skipping: no mail.db found");
        return;
    }
    let conn =
        rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("Failed to open mail.db");

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
        .expect("Failed to query messages");

    println!("mail.db: {} messages", count);
    assert!(count > 0, "Expected mail from Phase 0 agent coordination");

    // Read a sample message
    let mut stmt = conn
        .prepare("SELECT from_agent, to_agent, subject, type FROM messages ORDER BY created_at DESC LIMIT 3")
        .expect("Failed to prepare");
    let rows: Vec<(String, String, String, String)> = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .expect("Failed to query")
        .filter_map(|r| r.ok())
        .collect();

    for (from, to, subject, typ) in &rows {
        println!(
            "  {} -> {}: [{}] {}",
            from,
            to,
            typ,
            &subject[..subject.len().min(60)]
        );
    }
}

#[test]
fn read_real_events_db() {
    let db_path = Path::new(".overstory/events.db");
    if !db_path.exists() {
        eprintln!("Skipping: no events.db found");
        return;
    }
    let conn =
        rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("Failed to open events.db");

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .expect("Failed to query events");

    println!("events.db: {} events", count);
    assert!(count > 0, "Expected events from Phase 0 build");

    // Count by event type
    let mut stmt = conn
        .prepare("SELECT event_type, COUNT(*) FROM events GROUP BY event_type ORDER BY COUNT(*) DESC LIMIT 10")
        .expect("Failed to prepare");
    let rows: Vec<(String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("Failed to query")
        .filter_map(|r| r.ok())
        .collect();

    for (event_type, n) in &rows {
        println!("  {}: {}", event_type, n);
    }
}

#[test]
fn read_real_metrics_db() {
    let db_path = Path::new(".overstory/metrics.db");
    if !db_path.exists() {
        eprintln!("Skipping: no metrics.db found");
        return;
    }
    let conn =
        rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("Failed to open metrics.db");

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
        .expect("Failed to query metric sessions");

    println!("metrics.db: {} session records", count);

    // Total cost
    let total_cost: f64 = conn
        .query_row(
            "SELECT COALESCE(SUM(estimated_cost_usd), 0) FROM sessions",
            [],
            |row| row.get(0),
        )
        .expect("Failed to sum cost");

    println!("  total cost: ${:.2}", total_cost);
}

#[test]
fn config_loads_from_real_overstory_dir() {
    let config_path = Path::new(".overstory/config.yaml");
    assert!(config_path.exists(), ".overstory/config.yaml must exist");

    let content = std::fs::read_to_string(config_path).expect("Failed to read config");
    let value: serde_yaml::Value = serde_yaml::from_str(&content).expect("Failed to parse YAML");

    let project_name = value["project"]["name"]
        .as_str()
        .expect("project.name missing");
    println!("config: project.name = {}", project_name);
    assert_eq!(project_name, "grove");

    let quality_gates = value["project"]["qualityGates"]
        .as_sequence()
        .expect("qualityGates missing");
    println!("config: {} quality gates", quality_gates.len());
    assert!(quality_gates.len() >= 2);
}
