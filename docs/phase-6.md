# Phase 6: TUI Dashboard

## Context

Phases 0-5 are complete. Grove has full feature parity with overstory — every command is implemented and verified. This phase builds the terminal UI dashboard that makes grove a visual experience, not just a CLI.

This is grove's showcase feature. The ratatui TUI should feel like lazygit, bottom, or k9s — a real terminal application, not a log viewer.

## Architecture

The TUI is a single Rust binary (part of grove). `grove dashboard` launches it. It reads the same databases all other commands use (sessions.db, mail.db, events.db, metrics.db, merge-queue.db) and polls them on a 1-second tick.

**Framework:** ratatui + crossterm (already in Cargo.toml)

**No tmux required.** The TUI runs in the user's terminal directly.

## Views

### 1. Overview (default view)

The main dashboard. Shows everything at a glance.

**Layout:**
```
┌─────────────────────────────────────────────────────────┐
│ grove dashboard v0.1.0 │ run: run-2026... │ 3 agents │ $12.50 │ 15:04:32 │
├─────────────────────────────────────────────────────────┤
│ AGENTS (3)                                               │
│ St  Name              Capability   State     Duration  $ │
│ ●  builder-types      builder      working   3m 21s  $2 │
│ ●  config-builder     builder      working   2m 14s  $1 │
│ ✓  types-lead         lead         completed 5m 02s  $8 │
├──────────────────────────────────┬──────────────────────┤
│ FEED (live)                      │ MAIL (3 unread)      │
│ 15:04:31 builder-types Edit ...  │ ● lead → coord: done │
│ 15:04:30 config-builder Bash ... │   coord → lead: task │
│ 15:04:28 builder-types Bash ...  │   system: group done │
├──────────────────────────────────┴──────────────────────┤
│ MERGE: 0 pending │ METRICS: 3 sessions, $12.50 total    │
├─────────────────────────────────────────────────────────┤
│ [q]quit [?]help [tab]focus [↵]detail [/]filter [r]refresh│
└─────────────────────────────────────────────────────────┘
```

**Panels:**
- Header bar: full-width colored background, project name, run ID, agent count, total cost, time
- Agent table: the primary panel. Columns: state icon, name, capability, state, task, duration, cost. Selected row highlighted. Sorted by state priority (working > booting > stalled > zombie > completed)
- Feed panel: live event stream from events.db. Parse tool_args to show: tool name + first arg + duration. Color-code by agent.
- Mail panel: recent messages with unread indicator (●), from→to, subject, relative time
- Merge/metrics bar: single-line status strips
- Key hints: bottom bar showing available keys with highlighted badges

**Keyboard:**
- `tab` / `shift+tab`: cycle focus between panels (agents, feed, mail)
- `↑/↓` or `j/k`: navigate within focused panel
- `enter`: drill into selected item (agent detail view, mail read view)
- `q`: quit
- `?`: help overlay
- `/`: filter input (filters agent table by name)
- `r`: force refresh
- `1/2/3`: switch views (overview, event log, help)
- `a`: show all agents (including completed), toggle

### 2. Agent Detail View (enter on agent)

Deep dive into a single agent.

**Layout:**
```
┌─────────────────────────────────────────────────────────┐
│ ← Agent: builder-types (builder)                         │
├──────────────────────────┬──────────────────────────────┤
│ SESSION                  │ TOKENS                        │
│ Task: grove-c146         │ Input:   12,450               │
│ Branch: overstory/...    │ Output:  3,221                │
│ Worktree: .overstory/... │ Cache:   890,123              │
│ State: working           │ Cost:    $2.14                │
│ PID: 12345               │ Model:   sonnet               │
│ Started: 3m 21s ago      │                               │
├──────────────────────────┴──────────────────────────────┤
│ RECENT EVENTS                                            │
│ 15:04:31 Edit src/types.rs (320ms)                       │
│ 15:04:28 Bash cargo build (4521ms)                       │
│ 15:04:20 Read src/config.rs                              │
├─────────────────────────────────────────────────────────┤
│ MAIL (sent/received)                                     │
│ ● orchestrator → builder-types: Dispatch grove-c146      │
│   builder-types → types-lead: worker_done                │
├─────────────────────────────────────────────────────────┤
│ [esc]back [↑↓]scroll [t]tmux attach                     │
└─────────────────────────────────────────────────────────┘
```

- `esc` or `backspace`: return to overview
- `t`: attach to agent's tmux session (if exists) — runs `tmux attach -t <session>` replacing the TUI process, which resumes when user detaches

### 3. Event Log View

Full-screen scrollable event log with filtering.

- Shows all events from events.db
- Filter by agent, event type, level
- `/` to search
- Scrollable with vim keys

### 4. Help Overlay

Transparent overlay showing all keyboard shortcuts. Toggled with `?`.

## Data Sources

All data comes from the existing database layer (`src/db/`):

- **Agent table:** `SessionStore::get_all()` — poll every 1s
- **Feed:** `EventStore::get_timeline()` — poll every 1s, use ID cursor for new events only
- **Mail:** `MailStore::get_all()` — poll every 2s
- **Merge bar:** `MergeQueue::list()` — poll every 5s
- **Metrics bar:** `MetricsStore::count_sessions()` + `MetricsStore::get_total_cost()` — poll every 5s
- **Token costs:** `MetricsStore::get_latest_snapshot()` per agent — poll every 3s
- **Run ID:** Read `.overstory/current-run.txt`

## Implementation Structure

```
src/tui/
├── mod.rs           — public API: launch_dashboard()
├── app.rs           — App state, event loop, input handling
├── views/
│   ├── mod.rs
│   ├── overview.rs  — main dashboard view
│   ├── agent_detail.rs — drill-in agent view
│   ├── event_log.rs — full-screen event log
│   └── help.rs      — help overlay
├── widgets/
│   ├── mod.rs
│   ├── agent_table.rs — styled agent table with selection
│   ├── feed.rs        — live event feed widget
│   ├── mail_list.rs   — mail panel widget
│   ├── status_bar.rs  — key hints bar
│   └── header.rs      — header bar widget
└── theme.rs         — ratatui styles, colors, brand palette
```

## Theme

Match grove's brand from `src/logging/mod.rs`:

- **BrandGreen** (#2e7d32) — focused borders, active states
- **AccentAmber** (#ffb74d) — highlights, agent names
- **MutedGray** (#78786e) — dim text, completed agents
- **WorkingColor** (#4caf50) — working state
- **BootingColor** (#ffc107) — booting state
- **StalledColor** (#f44336) — stalled/error state
- **CompletedColor** (#00bcd4) — completed state

Panel borders: `ratatui::widgets::Block` with `Borders::ALL` and `BorderType::Rounded`
Focused panel: BrandGreen border
Unfocused panel: dark gray (#3a3a3a) border

## File Scope

New files:
- `src/tui/mod.rs`
- `src/tui/app.rs`
- `src/tui/theme.rs`
- `src/tui/views/mod.rs`
- `src/tui/views/overview.rs`
- `src/tui/views/agent_detail.rs`
- `src/tui/views/event_log.rs`
- `src/tui/views/help.rs`
- `src/tui/widgets/mod.rs`
- `src/tui/widgets/agent_table.rs`
- `src/tui/widgets/feed.rs`
- `src/tui/widgets/mail_list.rs`
- `src/tui/widgets/status_bar.rs`
- `src/tui/widgets/header.rs`

Modified files:
- `src/commands/mod.rs` — register tui module if not already
- `src/main.rs` — wire `Commands::Dashboard` to `tui::launch_dashboard()`

## Quality Gates

- `cargo build` — clean
- `cargo test` — all existing tests pass + new TUI tests (widget rendering, state transitions)
- `cargo clippy -- -D warnings`

## Verification Commands

```bash
G=./target/debug/grove

# 1. Dashboard launches and renders
# (Can't fully automate TUI testing, but verify it doesn't crash)
timeout 3 $G dashboard 2>&1; echo "EXIT: $?"
# Should exit 0 (timeout) or show TUI, not crash

# 2. Dashboard not a stub
$G dashboard 2>&1 | head -1 | grep -q "not yet implemented" && echo "FAIL: still stub" || echo "PASS: dashboard wired"

# 3. Verify TUI module compiles
cargo build && echo "BUILD PASS"

# 4. TUI tests pass
cargo test tui && echo "TUI TESTS PASS"

# Quality gates
cargo test && echo "ALL TESTS PASS"
cargo clippy -- -D warnings && echo "CLIPPY PASS"
```

## Acceptance Criteria

1. `grove dashboard` launches a full-screen TUI that renders without crashing
2. Overview shows: header bar, agent table, feed panel, mail panel, merge/metrics bars, key hints
3. Agent table updates every 1s with live data from sessions.db
4. Feed panel shows live events from events.db
5. Mail panel shows recent messages with unread indicators
6. Keyboard navigation works: tab to focus, j/k to navigate, enter for detail view, q to quit
7. Agent detail view shows session info, tokens, events, mail for selected agent
8. `esc` returns from detail view to overview
9. Help overlay toggles with `?`
10. Filter input with `/` filters agent table
11. Theme uses grove brand colors consistently
12. All existing tests pass
13. No panics or crashes on empty database state (fresh project with no agents)
