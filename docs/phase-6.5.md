# Phase 6.5: TUI Enhancements — Live Agent Terminals + Rich Content

## Context

Phase 6 delivered the core TUI dashboard (2,164 lines, 14 files). It works: agent table, feed panel, mail panel, event log, keyboard navigation. But it's a monitoring dashboard, not an interactive experience.

This phase makes it feel like a proper terminal multiplexer for AI agents. The killer features: watching agent terminals live, split-pane views, and rich browsable content.

## Deliverables

### 1. Fix Phase 6 Bugs

**BUG-1: Help overlay doesn't render.**
`show_help` is set to `true` by `?` but the draw method doesn't render an overlay widget on top of the current view. Fix: in `app.rs` `draw()`, if `show_help` is true, render a `Clear` + centered `Paragraph` overlay on top of whatever view is active. Use `ratatui::widgets::Clear` to punch through the background.

**BUG-2: Event log view is sparse.**
The event log view (view `2`) only shows 1 event despite having 500+. Likely the scroll offset or query limit is wrong. Fix: ensure `EventStore::get_timeline()` is called with the right limit and the scroll state iterates over all results.

### 2. Agent Terminal Viewer (new view)

The headliner. Press `t` on a selected agent to open a live terminal view showing what that agent is doing.

**Implementation:**
- New view: `View::Terminal` in `src/tui/views/terminal.rs`
- Reads the agent's tmux session content using `tmux capture-pane -t <session> -p -S -100`
- Polls every 500ms for fresh content
- Renders as a scrollable ANSI-aware text block
- The terminal view should preserve ANSI colors from the tmux capture (use `ansi-to-tui` crate or strip to plain text)
- Header shows agent name, state, and "[esc] back [f] fullscreen [s] split"

**Keyboard in terminal view:**
- `esc` — back to overview
- `j/k` or `↑/↓` — scroll
- `f` — toggle fullscreen (hides header/status bar)
- `s` — split view (see below)

### 3. Split Terminal View

Watch 2-4 agent terminals simultaneously.

**Implementation:**
- New view: `View::SplitTerminal` in `src/tui/views/split_terminal.rs`
- Press `s` from terminal view to enter split mode
- Auto-selects all working agents (up to 4)
- Renders 2x2 grid (or 1x2 if only 2 agents) of terminal panels
- Each panel shows tmux capture from one agent
- Tab cycles focus between panels
- `enter` on a focused panel goes to full terminal view for that agent
- Number keys `1-4` directly focus panels

**Layout:**
```
┌─────────────────────────┬─────────────────────────┐
│ builder-types [working] │ config-builder [working] │
│ $ cargo build           │ ● Editing src/config.rs  │
│   Compiling grove v0.1  │   ... 42 lines changed   │
│   ...                   │   ...                     │
├─────────────────────────┼─────────────────────────┤
│ types-lead [working]    │ (empty)                   │
│ $ ov status             │                           │
│   3 agents active       │                           │
│   ...                   │                           │
└─────────────────────────┴─────────────────────────┘
```

### 4. Enhanced Agent Detail View

The current agent detail view is bare. Enhance it:

- **Transcript timeline:** Show the last 20 tool calls with durations, color-coded by type (Bash=blue, Edit=yellow, Read=green)
- **Live cost tracker:** Show token counts updating in real-time from metrics snapshots
- **File activity:** Show recently modified files from Edit events (parse file_path from event args)
- **Status timeline:** Show state transitions (booting → working → completed) with timestamps

### 5. Mail Reader View

Press `enter` on a mail item to read the full message.

- Renders full message body in a scrollable panel
- Shows threading (replies indented)
- `r` to compose a reply (opens a text input at the bottom)
- `esc` to go back

### 6. Rich Feed Panel

The feed panel currently shows raw JSON args. Parse them into human-readable lines:

- `Bash {"command":"cargo build"}` → `$ cargo build`
- `Edit {"file_path":"src/main.rs"}` → `✎ src/main.rs`
- `Read {"file_path":"src/config.rs"}` → `📖 src/config.rs`  
- `Write {"file_path":"src/types.rs"}` → `💾 src/types.rs`
- `session_start` → `▶ started`
- `session_end` → `⏹ completed`
- `mail_sent` → `✉ mail sent`
- Color-code each agent's events with a consistent color from a palette

### 7. Status Bar Improvements

- Show system load (read from `/proc/loadavg`)
- Show disk usage (read from `df`)
- Show git branch and last commit
- Flash notifications: "Agent X completed!" fades after 3s

## New Dependencies

Add to Cargo.toml if needed:
- `ansi-to-tui` — for rendering ANSI terminal content in ratatui (optional, can strip to plain text instead)

## File Scope

New files:
- `src/tui/views/terminal.rs` — single agent terminal viewer
- `src/tui/views/split_terminal.rs` — multi-agent split view
- `src/tui/views/mail_reader.rs` — full mail reader

Modified files:
- `src/tui/app.rs` — new views, help overlay fix, event log fix, split logic
- `src/tui/views/mod.rs` — register new views
- `src/tui/views/overview.rs` — rich feed rendering
- `src/tui/views/agent_detail.rs` — enhanced detail content
- `src/tui/views/help.rs` — update keybindings in help text
- `src/tui/views/event_log.rs` — fix sparse rendering
- `src/tui/widgets/feed.rs` — parse tool args into human-readable format
- `src/tui/widgets/status_bar.rs` — system stats + notifications
- `src/tui/widgets/header.rs` — git branch info

## Verification Commands

```bash
G=./target/debug/grove

# 1. Help overlay renders (manual check in tmux)
tmux new-session -d -s tui-test "$G dashboard" && sleep 2
tmux send-keys -t tui-test '?' && sleep 1
tmux capture-pane -t tui-test -p | grep -i "help\|keyboard\|shortcut" && echo "HELP PASS" || echo "HELP FAIL"
tmux send-keys -t tui-test 'q'

# 2. Event log shows multiple events
tmux new-session -d -s tui-test2 "$G dashboard" && sleep 2
tmux send-keys -t tui-test2 '2' && sleep 1
EVENT_COUNT=$(tmux capture-pane -t tui-test2 -p | grep -c "20[0-9][0-9]")
[ "$EVENT_COUNT" -gt 5 ] && echo "EVENT LOG PASS ($EVENT_COUNT events)" || echo "EVENT LOG FAIL ($EVENT_COUNT events)"
tmux send-keys -t tui-test2 'q'

# 3. Feed shows parsed format (not raw JSON)
tmux new-session -d -s tui-test3 "$G dashboard" && sleep 2
FEED=$(tmux capture-pane -t tui-test3 -p)
echo "$FEED" | grep -qE '^\$|✎|📖' && echo "RICH FEED PASS" || echo "RICH FEED FAIL (may still show raw)"
tmux send-keys -t tui-test3 'q'

# 4. Build + tests
cargo build && cargo test && cargo clippy -- -D warnings && echo "ALL PASS"
```

## Acceptance Criteria

1. Help overlay renders centered on top of current view when `?` is pressed
2. Event log view shows all events with scrolling
3. `t` on selected agent opens live terminal view showing tmux content
4. `s` from terminal view opens split view with up to 4 agent terminals
5. Feed panel shows human-readable tool descriptions, not raw JSON
6. Agent detail view shows transcript timeline, live costs, file activity
7. Enter on mail opens full message reader
8. Status bar shows system load and git info
9. All existing tests pass + new tests for new views
10. No panics on agents without tmux sessions
