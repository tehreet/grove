# Phase 0: Foundation

Build the foundation that every subsequent phase depends on: types, config, errors, database layer, CLI skeleton, and logging.

## Deliverables

### 1. `src/types.rs` — All shared types

Port all 72 types from `reference/types.ts`. Every type needs `#[derive(Debug, Clone, Serialize, Deserialize)]`. Enums with string representations need custom serde (e.g., `AgentState` serializes as `"working"`, not `"Working"`).

Key types to port:
- `OverstoryConfig` (the big one — nested project/agents/worktrees/mulch/merge/watchdog/coordinator/models/runtime config)
- `AgentSession`, `AgentState`, `AgentIdentity`
- `Capability` (enum: builder, scout, reviewer, verifier, lead, merger, coordinator, supervisor, orchestrator, monitor)
- `MailMessage`, `MailMessageType`, `MailPriority`
- `MergeEntry`, `MergeResult`, `ResolutionTier`
- `QualityGate`, `VerificationConfig`
- `SessionMetrics`, `TokenSnapshot`
- `StoredEvent`, `EventType`
- All overlay config types

Use `#[serde(rename_all = "camelCase")]` where the JSON uses camelCase. Use `#[serde(rename = "snake_case")]` for database columns.

### 2. `src/config.rs` — YAML config loader

Port `reference/config.ts`. Must:
- Find `.overstory/config.yaml` by walking up from CWD (same logic as TypeScript)
- Merge with `config.local.yaml` if present
- Apply `DEFAULT_CONFIG` for missing fields
- Validate (quality gate commands not empty, paths exist, etc.)
- Support `--project` global flag override

### 3. `src/errors.rs` — Error types

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GroveError {
    #[error("Config error: {message}")]
    Config { message: String, field: Option<String> },
    
    #[error("Agent error: {message}")]
    Agent { message: String, agent: Option<String> },
    
    #[error("Worktree error: {message}")]
    Worktree { message: String },
    
    #[error("Merge error: {message}")]
    Merge { message: String, tier: Option<String> },
    
    #[error("Validation error: {message}")]
    Validation { message: String, field: Option<String> },
    
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
```

### 4. `src/db/connection.rs` — Shared database opener

```rust
pub fn open_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(conn)
}

pub fn open_db_readonly(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "busy_timeout", 3000)?;
    Ok(conn)
}
```

### 5. `src/db/sessions.rs` — Session store

Port `reference/sessions-store.ts`. Methods:
- `create_tables()` — idempotent schema creation
- `register(session: &AgentSession)` → insert
- `get_all()` → Vec<AgentSession>
- `get_by_name(name: &str)` → Option<AgentSession>
- `update_state(name: &str, state: AgentState)`
- `update_activity(name: &str)`
- `close()` — connection cleanup

### 6. `src/db/mail.rs` — Mail store

Port `reference/mail-store.ts`. Methods:
- `create_tables()`
- `insert(msg: &MailMessage)`
- `get_all(filters: MailFilters)` → Vec<MailMessage>
- `get_unread(to: &str)` → Vec<MailMessage>
- `get_by_id(id: &str)` → Option<MailMessage>
- `mark_read(id: &str)`

### 7. `src/db/events.rs` — Event store

Port `reference/events-store.ts`. Methods:
- `create_tables()`
- `insert(event: &StoredEvent)`
- `get_timeline(since: &str, limit: usize)` → Vec<StoredEvent>
- `get_by_agent(agent: &str, limit: usize)` → Vec<StoredEvent>

### 8. `src/db/metrics.rs` — Metrics store

Port `reference/metrics-store.ts`. Methods:
- `create_tables()`
- `record_session(session: &SessionMetrics)`
- `record_snapshot(snapshot: &TokenSnapshot)`
- `get_recent_sessions(limit: usize)` → Vec<SessionMetrics>
- `get_sessions_by_run(run_id: &str)` → Vec<SessionMetrics>
- `count_sessions()` → usize
- `get_average_duration()` → f64

### 9. `src/db/merge_queue.rs` — Merge queue

Port `reference/merge-queue.ts`. Methods:
- `create_tables()`
- `enqueue(entry: &MergeEntry)`
- `dequeue()` → Option<MergeEntry>
- `list(status: Option<&str>)` → Vec<MergeEntry>
- `update_status(id: i64, status: &str, tier: Option<&str>)`

### 10. `src/logging/mod.rs` — Terminal output

Port `reference/color.ts` + `reference/theme.ts`. Implement:
- Brand palette: BrandGreen (#2e7d32), AccentAmber (#ffb74d), MutedGray (#78786e)
- `print_success()`, `print_error()`, `print_warning()`, `print_hint()`
- State colors and icons (working=green >, booting=yellow ~, stalled=red !, zombie=gray x, completed=cyan ✓)
- `strip_ansi()`, `visible_length()`
- `format_duration()`, `format_relative_time()`

### 11. `src/main.rs` — CLI skeleton with clap

Port the command structure from `reference/index.ts`. All 35 commands defined as clap subcommands. Each prints "not yet implemented" for now. The structure:

```rust
#[derive(Parser)]
#[command(name = "grove", about = "Multi-agent orchestration for AI coding agents")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    #[arg(long, global = true)]
    project: Option<PathBuf>,
    
    #[arg(short, long, global = true)]
    quiet: bool,
    
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    Init { ... },
    Sling { task_id: String, ... },
    Status { ... },
    Dashboard { ... },
    Mail { #[command(subcommand)] action: MailAction },
    Coordinator { #[command(subcommand)] action: CoordinatorAction },
    // ... all 35 commands
}
```

### 12. `src/json.rs` — JSON output helpers

Port `reference/json.ts`. Standardized JSON envelope:
```rust
pub fn json_output<T: Serialize>(command: &str, data: &T) {
    let envelope = serde_json::json!({
        "command": command,
        "data": data,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    println!("{}", serde_json::to_string_pretty(&envelope).unwrap());
}
```

## File Scope

New files:
- `src/main.rs`
- `src/types.rs`
- `src/config.rs`
- `src/errors.rs`
- `src/json.rs`
- `src/db/mod.rs`
- `src/db/connection.rs`
- `src/db/sessions.rs`
- `src/db/mail.rs`
- `src/db/events.rs`
- `src/db/metrics.rs`
- `src/db/merge_queue.rs`
- `src/logging/mod.rs`

## Quality Gates

- `cargo build` — clean compilation, zero warnings
- `cargo test` — all unit tests pass (write tests for config parsing, database CRUD, type serialization)
- `cargo clippy` — no warnings
- `cargo fmt --check` — formatted

## Acceptance Criteria

1. `grove --help` prints all 35 commands with descriptions
2. `grove status` prints "not yet implemented" (stub)
3. All types round-trip through serde_json (serialize → deserialize → equal)
4. Config loads from a sample `.overstory/config.yaml` and applies defaults correctly
5. Each database store can create tables, insert, and query on an in-memory SQLite database
6. `print_success`, `print_error` etc. produce colored output matching the overstory brand
7. Zero clippy warnings, formatted with rustfmt
