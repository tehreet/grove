# Phase 4: Coordinator + Required Observability Commands

## Context

Phases 0-3 are complete and verified. We have 15,017 lines of Rust, 306 tests, 16+ working commands. All 7 bugs from integration testing are fixed. Grove can spawn agents, manage sessions, send mail, and interoperate with overstory.

This phase implements the commands the coordinator needs plus the coordinator itself.

## Part A: Prerequisite Commands

These must work before the coordinator can function.

### 1. `grove agents` — List agent definitions

```
grove agents [--json]
```

Read `.overstory/agent-manifest.json` and list all defined agent capabilities with their properties (file, can_spawn, model overrides). Reference: the manifest is already parsed by `src/agents/manifest.rs`.

### 2. `grove worktree list` — List active worktrees

```
grove worktree list [--json]
grove worktree clean [--completed] [--force]
```

- `list`: Enumerate `.overstory/worktrees/*/`, show branch name, HEAD, associated agent from sessions.db
- `clean --completed`: Remove worktrees for completed/zombie sessions. Delete the git branch too.
- `clean --force`: Also remove worktrees for agents not in sessions.db (orphaned)

Reference: `src/worktree/git.rs` already has `list_worktrees()`, `remove_worktree()`, `delete_branch()`.

### 3. `grove group` — Task group management

```
grove group list [--json]
grove group create <name> [--issues <csv>]
grove group add <group-id> <issue-ids...>
grove group status <group-id> [--json]
grove group close <group-id>
```

Groups are stored in `.overstory/groups.json`. A group tracks a set of related task IDs and their completion status. The coordinator uses groups to know when a phase of work is done.

Structure:
```json
{
  "groups": {
    "group-abc123": {
      "name": "phase-0-foundation",
      "status": "active",
      "issues": ["grove-c146", "grove-c37a", "grove-3e48"],
      "created_at": "2026-03-09T...",
      "closed_at": null
    }
  }
}
```

### 4. `grove run` — Run management

```
grove run list [--json]
grove run show <run-id> [--json]
```

Reads the `runs` table in sessions.db. Shows run ID, start time, agent count, status.

### 5. `grove feed` — Event stream

```
grove feed [--follow] [--agent <name>] [--type <type>] [--limit <n>]
```

Query events.db and display chronological event stream. With `--follow`, poll for new events every second (use the `id` column as cursor).

### 6. `grove errors` — Aggregated errors

```
grove errors [--agent <name>] [--limit <n>] [--json]
```

Query events.db for events with `level = 'error'`. Group by agent, show count and latest error per agent.

### 7. `grove inspect` — Deep agent inspection

```
grove inspect <agent-name> [--json]
```

Show everything about an agent: session record, recent events, mail sent/received, token usage from metrics.db, worktree status.

### 8. `grove trace` — Event timeline

```
grove trace <agent-name-or-task-id> [--json]
```

Chronological event timeline for a specific agent or task. Include mail, tool events, state changes.

## Part B: Coordinator

### 9. `grove coordinator start` — Native event loop

This is NOT an LLM session like overstory. This is a Rust event loop.

```
grove coordinator start [--no-attach] [--profile <delivery|co-creation>]
grove coordinator stop
grove coordinator status [--json]
grove coordinator send --subject <s> --body <b>
```

**start:**
1. Register coordinator session in sessions.db
2. Create tmux session named `overstory-{project}-coordinator`
3. Start the event loop inside that tmux session
4. The event loop runs forever (or until exit triggers):

```rust
loop {
    // 1. Check mail for coordinator
    let messages = mail_store.get_unread("coordinator");
    for msg in messages {
        handle_message(msg); // dispatch tasks, respond to status updates
    }
    
    // 2. Check for completed agents
    let completed = session_store.get_completed_since(last_check);
    for agent in completed {
        handle_completion(agent); // check if group is done, trigger merges
    }
    
    // 3. Check merge queue
    let pending = merge_queue.get_pending();
    for entry in pending {
        handle_merge(entry); // run grove merge
    }
    
    // 4. Check exit triggers
    if config.coordinator.exit_triggers.all_agents_done {
        let active = session_store.get_active();
        if active.is_empty() { break; }
    }
    
    // 5. Sleep
    sleep(Duration::from_secs(1));
}
```

**The key difference from overstory:** The coordinator does NOT call an LLM for orchestration decisions. It's deterministic. The LLM is only called for task decomposition — when a `dispatch` mail arrives with a task description, the coordinator calls Claude API (one-shot, via `reqwest`) to decompose it into subtasks, then spawns leads via `grove sling`.

**send:**
Insert a message to the coordinator's mailbox. This is how the human (or Claude.ai) sends work to the coordinator.

**stop:**
Send SIGTERM to the coordinator process, update session state.

**status:**
Show coordinator session info, current group status, active agents.

## File Scope

New files:
- `src/commands/agents.rs`
- `src/commands/worktree.rs` (the command, not src/worktree/)
- `src/commands/group.rs`
- `src/commands/run.rs`
- `src/commands/feed.rs`
- `src/commands/errors.rs`
- `src/commands/inspect.rs`
- `src/commands/trace.rs`
- `src/commands/coordinator.rs`
- `src/coordinator/mod.rs`
- `src/coordinator/event_loop.rs`
- `src/coordinator/planner.rs`

Modified files:
- `src/main.rs` — wire all new commands (VERIFY: no not_yet_implemented stubs for these commands after this phase)
- `src/commands/mod.rs` — register modules

## Quality Gates

- `cargo build` — clean
- `cargo test` — all existing 306 tests pass + new tests
- `cargo clippy -- -D warnings`

## Verification Commands

Run these AFTER implementation to verify (lesson from RETRO-001):

```bash
# Part A
grove agents --json | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'agents: {len(d)}')"
grove worktree list
grove group list
grove run list
grove feed --limit 5
grove errors --limit 5
grove inspect coordinator 2>&1 | head -5  # should show something, not stub
grove trace coordinator 2>&1 | head -5

# Part B
grove coordinator status
grove coordinator start --no-attach
sleep 5
grove coordinator send --subject "test" --body "Hello coordinator"
sleep 5
grove coordinator status  # should show working
grove mail check --agent coordinator  # should show the test message
grove coordinator stop
grove coordinator status  # should show completed

# No stubs left for these commands
for cmd in agents "worktree list" "group list" "run list" "feed --limit 1" "errors --limit 1" coordinator; do
  result=$(grove $cmd 2>&1 | head -1)
  if echo "$result" | grep -q "not yet implemented"; then
    echo "FAIL: grove $cmd is still a stub"
  fi
done
```

## Acceptance Criteria

1. All Part A commands produce meaningful output (not stubs)
2. `grove coordinator start --no-attach` launches a persistent event loop
3. `grove coordinator send` delivers messages to the coordinator
4. `grove coordinator stop` cleanly terminates the coordinator
5. The coordinator event loop checks mail, handles completions, and evaluates exit triggers
6. All JSON outputs include "success" and "command" fields
7. All 306 existing tests still pass
8. No `not_yet_implemented` stubs remain for any Phase 4 command
