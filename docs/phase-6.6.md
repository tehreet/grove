# Phase 6.6: TUI Polish — Cost Analytics, Timeline, Agent Cards

## Context

Phase 6.5 adds terminal viewer, split view, rich feed, mail reader. This phase adds the analytical and visual polish that makes grove's TUI best-in-class.

## Deliverables

### 1. Agent Cards (Overview Redesign)

Replace the agent table rows with visual cards. Each card shows:
- Agent name + capability badge (colored pill)
- Current activity: "editing src/types.rs" or "$ cargo build (4.2s)" 
- Mini terminal preview: last 3 lines of tmux capture
- Progress bar: files touched / commits made
- Token burn: input/output tokens + cost
- State indicator with color glow

Cards arranged in a responsive grid — 2 columns on wide terminals, 1 on narrow.

### 2. Cost Analytics Panel

New view (press `$` or `4`):
- Per-agent cost breakdown (bar chart using ratatui)
- Burn rate: $/minute for the current run
- Projected total cost based on burn rate
- Cost comparison: "Switching builders to Codex would save ~$X"
- Historical cost per phase (if metrics data exists)

### 3. Timeline / Gantt View

New view (press `5` or `g`):
- Horizontal timeline, one row per agent
- Colored bars showing: booting (yellow), working (green), stalled (red), completed (purple)
- Time axis at top
- Shows gaps and overlaps — easy to spot stuck agents
- Current time marker

### 4. Toast Notifications

- Agent completed → brief green toast top-right
- Agent errored/zombied → red toast
- Mail received → purple toast
- Merge completed → cyan toast
- Toasts fade after 3 seconds
- Stack up to 3 toasts

### 5. Agent Mini-Terminal in Overview

In the overview's feed panel area, show a rotating preview of the most recently active agent's terminal (last 5 lines). Updates every second. Gives you a sense of what's happening without entering terminal view.

## Not in Scope

- Input/compose features (sending mail from TUI) — save for later
- Config editing from TUI
- Runtime switching from TUI
