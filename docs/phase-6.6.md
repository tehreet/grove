# Phase 6.6: TUI Polish — Theme, Cost Analytics, Timeline, Agent Cards, Toasts

## Context

Phase 6.5 delivered terminal viewer, split view, mail reader, rich feed, system stats. The TUI works but looks generic (green/amber theme). This phase makes it visually stunning and adds analytical depth.

## CRITICAL: Theme Update (applies to ALL work in this phase)

Replace the ENTIRE color palette in `src/tui/theme.rs`. Every widget, view, and panel must use these colors:

```rust
// Dracula + charm.sh inspired palette — vibrant pinks and purples
pub const BRAND_PRIMARY: Color = Color::Rgb(255, 85, 170);     // hot pink / magenta
pub const ACCENT_PURPLE: Color = Color::Rgb(189, 147, 249);    // lavender
pub const ACCENT_CYAN: Color = Color::Rgb(139, 233, 253);      // bright cyan
pub const ACCENT_GREEN: Color = Color::Rgb(80, 250, 123);      // bright green
pub const ACCENT_YELLOW: Color = Color::Rgb(241, 250, 140);    // soft yellow
pub const ACCENT_ORANGE: Color = Color::Rgb(255, 184, 108);    // soft orange
pub const ACCENT_RED: Color = Color::Rgb(255, 85, 85);         // bright red

pub const WORKING_COLOR: Color = ACCENT_GREEN;                  // bright green
pub const BOOTING_COLOR: Color = ACCENT_YELLOW;                 // soft yellow
pub const STALLED_COLOR: Color = ACCENT_RED;                    // bright red
pub const ZOMBIE_COLOR: Color = Color::Rgb(255, 85, 85);       // same red, dimmed in display
pub const COMPLETED_COLOR: Color = ACCENT_PURPLE;               // lavender

pub const MUTED: Color = Color::Rgb(98, 114, 164);             // dracula comment gray
pub const HEADER_BG: Color = Color::Rgb(40, 42, 54);           // dracula background
pub const BORDER_FOCUSED: Color = BRAND_PRIMARY;                // hot pink
pub const BORDER_UNFOCUSED: Color = Color::Rgb(68, 71, 90);    // dracula current line
pub const TEXT_PRIMARY: Color = Color::Rgb(248, 248, 242);      // dracula foreground
pub const TEXT_DIM: Color = Color::Rgb(98, 114, 164);           // dracula comment
```

Agent state icons:
```rust
Working => "▶"
Booting => "◌"
Stalled => "⚠"
Zombie  => "☠"
Completed => "✔"
```

This theme must be applied to ALL existing widgets (header, agent_table, feed, mail_list, status_bar) AND all new widgets created in this phase. No green/amber should remain anywhere.

## Deliverables

### 1. Theme Overhaul (MUST DO FIRST)
- Rewrite `src/tui/theme.rs` with the palette above
- Update all existing widgets to use new color constants
- Update all views to use new colors
- Agent names: hot pink
- Capability badges: cyan
- Timestamps: muted gray
- Active borders: hot pink
- Inactive borders: dracula current line
- Headers: pink on dark purple background
- Selected rows: dark purple background with bright text

### 2. Agent Cards (Overview Redesign)
Replace the agent table rows with visual card blocks. Each card:
- Agent name (hot pink, bold) + capability pill (cyan background)
- Current activity line: "✎ src/types.rs" or "$ cargo build (4.2s)" (parsed from latest event)
- Mini terminal: last 3 lines of agent output (muted, monospace)
- Stats line: tokens in/out, cost, duration
- State indicator: colored icon + text

Cards in responsive grid: 2 columns wide terminal, 1 column narrow.

### 3. Cost Analytics View (press `4` or `$`)
New view: `View::CostAnalytics` in `src/tui/views/cost_analytics.rs`
- Per-agent cost bars (horizontal bar chart, ratatui `BarChart` or custom spans)
- Burn rate: calculate $/minute from recent metrics snapshots
- Projected total: burn_rate * estimated_remaining_time
- Session cost table: agent, capability, tokens_in, tokens_out, cache, cost

### 4. Timeline / Gantt View (press `5` or `g`)
New view: `View::Timeline` in `src/tui/views/timeline.rs`
- One row per agent
- Horizontal bars colored by state (green=working, yellow=booting, red=stalled, purple=completed)
- Time axis at top, normalized to run duration
- Current time marker (vertical pink line)
- Shows overlap and gaps at a glance

### 5. Toast Notifications
In `src/tui/widgets/toasts.rs`:
- Render in top-right corner, overlaid on current view
- Agent completed → green toast "✔ builder-1 completed"
- Agent zombied → red toast "☠ builder-2 died"
- Mail received → purple toast "✉ new mail from lead-1"
- Toasts auto-dismiss after 3 seconds
- Stack up to 3, newest on top
- Track in `App.toasts: Vec<Toast>` with timestamp for expiry

### 6. Agent Mini-Terminal in Overview
Replace the current feed panel's bottom section with a mini-terminal showing the most recently active agent's last 5 lines. Rotates between agents every 5 seconds if multiple are active. Shows agent name and state above the preview.

## File Scope

**New files:**
- `src/tui/views/cost_analytics.rs`
- `src/tui/views/timeline.rs`
- `src/tui/widgets/toasts.rs`
- `src/tui/widgets/agent_card.rs`

**Modified files:**
- `src/tui/theme.rs` — COMPLETE REWRITE
- `src/tui/app.rs` — new views, toast state, agent card state
- `src/tui/views/mod.rs` — register new views
- `src/tui/views/overview.rs` — agent cards layout, mini-terminal
- `src/tui/views/help.rs` — update keybindings for new views
- `src/tui/widgets/mod.rs` — register new widgets
- `src/tui/widgets/agent_table.rs` — new colors
- `src/tui/widgets/feed.rs` — new colors
- `src/tui/widgets/header.rs` — pink on dark purple
- `src/tui/widgets/mail_list.rs` — new colors
- `src/tui/widgets/status_bar.rs` — new colors + toast rendering

**Integration (ONE builder owns this):**
- `src/main.rs` — NO CHANGES needed (dashboard already wired)
- `src/tui/app.rs` — this is the integration point, one builder only

## RETRO Lessons Applied

- **RETRO-008:** ONE builder owns `app.rs`. Other builders create new files and export render functions.
- **RETRO-028:** Theme spec is HERE, not in a mail sent mid-build.
- **RETRO-007/011:** No new commands to wire, but new views must be registered in `views/mod.rs` and `app.rs`.
- **RETRO-030:** Follow the Phase 6.5 success pattern — each builder owns distinct files.

## Suggested Decomposition (3 builders)

**Builder 1 — Theme + Existing Widget Updates (OWNS app.rs, theme.rs):**
- Rewrite theme.rs
- Update all existing widgets and views to new colors
- Register new views in app.rs and views/mod.rs
- Wire toast rendering in the draw loop
- Files: theme.rs, app.rs, views/mod.rs, all existing widget files

**Builder 2 — Agent Cards + Mini-Terminal + Toasts:**
- agent_card.rs widget
- toasts.rs widget
- Update overview.rs to use cards instead of table
- Mini-terminal in overview
- Files: widgets/agent_card.rs, widgets/toasts.rs, views/overview.rs

**Builder 3 — Cost Analytics + Timeline:**
- cost_analytics.rs view
- timeline.rs view
- Files: views/cost_analytics.rs, views/timeline.rs

## Verification Commands

```bash
G=./target/debug/grove

# Theme applied — no BRAND_GREEN remaining
grep -rn "BRAND_GREEN" src/tui/ | grep -v "theme.rs" | grep -v "test"
# Should return 0 lines (all references replaced)

# New views exist and compile
cargo build && echo "BUILD PASS"
cargo test tui && echo "TUI TESTS PASS"

# TUI renders with new colors (manual in tmux)
tmux new-session -d -s theme-test "$G dashboard"
sleep 3
tmux capture-pane -t theme-test -p | head -20
# VERIFY: visually distinct from old green theme
tmux send-keys -t theme-test '4'  # cost analytics
sleep 1
tmux capture-pane -t theme-test -p | head -5
tmux send-keys -t theme-test '5'  # timeline
sleep 1
tmux capture-pane -t theme-test -p | head -5
tmux send-keys -t theme-test 'q'

# Quality gates
cargo test && echo "ALL TESTS PASS"
cargo clippy -- -D warnings && echo "CLIPPY PASS"
```

## Acceptance Criteria

1. Theme.rs uses the Dracula+charm.sh palette — no green/amber remaining
2. All existing widgets render with new colors
3. Agent cards replace the table in overview
4. Cost analytics view shows per-agent costs and burn rate
5. Timeline view shows horizontal Gantt bars per agent
6. Toast notifications appear and auto-dismiss
7. Mini-terminal shows in overview for active agents
8. All existing tests pass + new tests for new views/widgets
9. `?` help overlay lists all new keybindings
