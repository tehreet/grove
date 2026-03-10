---
name: grove-develop
description: Use this skill whenever writing code for the grove codebase — adding commands, runtime adapters, DB stores, TUI views, or modifying existing grove functionality. Triggers include any mention of 'grove source code', 'src/', 'add a command to grove', 'new runtime adapter', 'grove internals', implementing features in the grove Rust project, or working in the grove repo. This skill teaches the codebase patterns so new code fits naturally.
---

# Grove Development

Patterns for contributing code to grove (27,000-line Rust CLI for multi-agent orchestration).

## Codebase Structure

```
src/
├── main.rs          # CLI entry (clap derive), all 35 command match arms
├── types.rs         # ALL shared types (serde derives)
├── config.rs        # YAML config loader
├── errors.rs        # thiserror typed errors
├── json.rs          # JSON output helpers
├── commands/        # One file per command (same pattern each time)
├── db/              # SQLite stores: sessions.rs, mail.rs, events.rs, metrics.rs, merge_queue.rs
├── runtimes/        # Adapters: claude.rs, codex.rs, gemini.rs, copilot.rs, registry.rs
├── coordinator/     # Event loop (event_loop.rs) + LLM planner (planner.rs)
├── merge/           # Tiered resolver (resolver.rs) + queue (queue.rs)
├── tui/             # app.rs + views/ + widgets/
├── watchdog/        # PID health monitoring
├── agents/          # Manifest loader + overlay renderer
├── process/         # Child process spawn + monitor
├── worktree/        # Git worktree create/remove
└── logging/         # Terminal formatting + brand colors
```

## Adding a New Command

Every command follows the same shape. Here's the pattern:

**1. Create `src/commands/my_cmd.rs`:**
```rust
//! `grove my-cmd` — one-line description.

use crate::config::resolve_project_root;

pub fn execute(
    /* args */
    json: bool,
    project_override: Option<&std::path::Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;
    let overstory = root.join(".overstory");

    // Open DB if needed
    // Do work
    // Output (json or text)

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        // tests here
    }
}
```

**2. Add to `src/commands/mod.rs`:**
```rust
pub mod my_cmd;
```

**3. Wire in `src/main.rs`:**

Add the clap struct:
```rust
#[derive(Parser, Debug)]
struct MyCmdArgs {
    #[arg(long)]
    json: bool,
    // ... other args
}
```

Add the enum variant:
```rust
enum Commands {
    // ... existing
    MyCmd(MyCmdArgs),
}
```

Add the match arm:
```rust
Commands::MyCmd(args) => {
    commands::my_cmd::execute(args.json, project.as_deref())
}
```

**This wiring step is critical.** It's the #1 thing agents forget (RETRO-007, RETRO-011). Always include main.rs in your changes.

## Adding a Runtime Adapter

Implement the `AgentRuntime` trait from `src/runtimes/mod.rs`:

```rust
// src/runtimes/my_runtime.rs
use super::{AgentRuntime, HooksDef, ReadyPhase, ReadyState, SpawnOpts};

pub struct MyRuntime;

impl AgentRuntime for MyRuntime {
    fn id(&self) -> &str { "my-runtime" }
    fn instruction_path(&self) -> &str { "MY_INSTRUCTIONS.md" }  // where overlay goes
    fn is_headless(&self) -> bool { true }

    fn build_headless_command(&self, opts: &SpawnOpts) -> Vec<String> {
        vec!["my-cli".into(), "-p".into(), 
             format!("Read {} and begin.", opts.instruction_path)]
    }

    fn deploy_config(&self, worktree: &Path, overlay_content: &str, _hooks: &HooksDef) -> Result<(), String> {
        // Write overlay to instruction_path
        if !overlay_content.is_empty() {
            std::fs::write(worktree.join(self.instruction_path()), overlay_content)
                .map_err(|e| format!("Failed to write: {e}"))?;
        }
        Ok(())
    }

    // ... detect_ready, build_interactive_command, build_env
}
```

Then register in `src/runtimes/registry.rs`:
```rust
"my-runtime" => Ok(Box::new(my_runtime::MyRuntime)),
```

And add `pub mod my_runtime;` to `src/runtimes/mod.rs`.

Key considerations:
- `instruction_path()` determines where the overlay gets written. Claude uses `.claude/CLAUDE.md`, Codex uses `AGENTS.md`, etc.
- `build_headless_command()` returns the argv for spawning. The agent reads its instructions from `instruction_path()`.
- If the runtime's CLI doesn't accept Anthropic model names (like "sonnet"), filter them out in `build_headless_command`.

## Adding a DB Store

Follow the pattern in `src/db/sessions.rs`:

```rust
pub struct MyStore {
    conn: rusqlite::Connection,
}

impl MyStore {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = rusqlite::Connection::open(db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS ...")?;
        Ok(Self { conn })
    }

    pub fn insert(&self, item: &MyItem) -> Result<String> { ... }
    pub fn get_by_id(&self, id: &str) -> Result<Option<MyItem>> { ... }
}
```

All stores use WAL mode and 5s busy timeout for concurrent access from multiple agent processes.

## Adding a TUI View

1. Create `src/tui/views/my_view.rs` with a `pub fn render(f: &mut Frame, area: Rect, app: &App)` function
2. Add `pub mod my_view;` to `src/tui/views/mod.rs`
3. Add a `View::MyView` variant to the enum in `src/tui/app.rs`
4. Wire key routing in `handle_key_overview()`: `KeyCode::Char('X') => self.current_view = View::MyView`
5. Wire rendering in `render()`: `View::MyView => views::my_view::render(f, chunks[1], self)`

## Quality Gates

Run all three before committing:
```bash
cargo build && cargo test && cargo clippy -- -D warnings
```

## Common Pitfalls

- **Forgetting main.rs wiring** — The #1 agent failure mode. Always check that your new command is in the clap enum AND the match arm.
- **Parallel cargo builds** — If tests or CI run multiple cargo commands, run them sequentially. Parallel cargo deadlocks on the lock file.
- **DB schema changes** — The SQLite schema must stay compatible with overstory. Don't rename columns or change types.
- **Types in wrong place** — ALL shared types go in `src/types.rs`, not in command files. Commands import from types.
- **Forgetting `--json` support** — Every command that outputs data should support `--json` for machine-readable output matching overstory's format.
