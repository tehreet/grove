# Grove Comprehensive Test Plan

## Testing Philosophy

AI-generated unit tests verify AI assumptions. Human E2E testing verifies actual behavior. We need both, and we've been under-investing in the latter.

This plan covers grove's testing strategy. Originally written post-Phase 6.5, updated as phases progress. Every test is executable — copy-paste the commands and run them.

---

## TIER 1: CRITICAL PATH — Must pass before any new phase

These test the workflow that actually runs when agents build software.

### T1.1: Full Agent Lifecycle (grove-native spawning)

**What:** Test grove's own process spawning, NOT overstory's tmux spawning. This is grove's core architectural advantage and has NEVER been E2E tested.

```bash
G=./target/debug/grove
cd /home/joshf/grove

# Spawn an agent through grove's native path
$G sling lifecycle-test --capability builder --name lifecycle-agent \
  --spec docs/phase-5.md --files src/main.rs \
  --skip-task-check --no-scout-check

# VERIFY: worktree created
[ -d .overstory/worktrees/lifecycle-agent ] && echo "PASS: worktree" || echo "FAIL"

# VERIFY: overlay not empty
[ -s .overstory/worktrees/lifecycle-agent/.claude/CLAUDE.md ] && echo "PASS: overlay" || echo "FAIL"

# VERIFY: overlay contains agent name, task, file scope
grep -q "lifecycle-agent" .overstory/worktrees/lifecycle-agent/.claude/CLAUDE.md && echo "PASS: agent name in overlay" || echo "FAIL"
grep -q "src/main.rs" .overstory/worktrees/lifecycle-agent/.claude/CLAUDE.md && echo "PASS: file scope in overlay" || echo "FAIL"

# VERIFY: settings.local.json has hooks
[ -s .overstory/worktrees/lifecycle-agent/.claude/settings.local.json ] && echo "PASS: settings" || echo "FAIL"

# VERIFY: git branch exists
git branch | grep -q "lifecycle-agent" && echo "PASS: branch" || echo "FAIL"

# VERIFY: status shows agent
$G status 2>&1 | grep -q "lifecycle-agent" && echo "PASS: status" || echo "FAIL"
$G status --json 2>&1 | python3 -c "
import sys,json
d=json.load(sys.stdin)
agents = [a for a in d['agents'] if a['agentName']=='lifecycle-agent']
assert len(agents)==1, 'agent not in status'
print('PASS: status json')
"

# State transitions
$G log session-start --agent lifecycle-agent
STATE=$($G status --json 2>&1 | python3 -c "import sys,json; d=json.load(sys.stdin); print([a['state'] for a in d['agents'] if a['agentName']=='lifecycle-agent'][0])")
[ "$STATE" = "working" ] && echo "PASS: working state" || echo "FAIL: state is $STATE"

$G log session-end --agent lifecycle-agent --exit-code 0
STATE=$($G status --json 2>&1 | python3 -c "import sys,json; d=json.load(sys.stdin); print([a['state'] for a in d['agents'] if a['agentName']=='lifecycle-agent'][0])")
[ "$STATE" = "completed" ] && echo "PASS: completed state" || echo "FAIL: state is $STATE"

# Clean
$G clean --worktrees --sessions --force
[ ! -d .overstory/worktrees/lifecycle-agent ] && echo "PASS: clean" || echo "FAIL: worktree still exists"
$G status 2>&1 | grep -q "No agents" && echo "PASS: clean status" || echo "FAIL"
```

### T1.2: Mail Round-Trip + Interop

```bash
G=./target/debug/grove
cd /home/joshf/grove

# Grove writes, grove reads
$G mail send --from agent-a --to agent-b --subject "Test mail" --body "Hello from grove"
$G mail list 2>&1 | grep -q "Test mail" && echo "PASS: grove→grove" || echo "FAIL"

# Grove writes, ov reads
ov mail list 2>&1 | grep -q "Test mail" && echo "PASS: grove→ov interop" || echo "FAIL"

# Ov writes, grove reads
ov mail send --to grove-reader --from ov-sender --subject "From OV" --body "Hello from TS"
$G mail list 2>&1 | grep -q "From OV" && echo "PASS: ov→grove interop" || echo "FAIL"

# Mail check shows unread
$G mail check --agent agent-b 2>&1 | grep -q "1 unread" && echo "PASS: unread count" || echo "FAIL"

# Read marks as read
MAIL_ID=$($G mail list --json 2>&1 | python3 -c "
import sys,json
try:
    d=json.load(sys.stdin)
    msgs=d.get('messages',d.get('data',{}).get('messages',[]))
    print(msgs[0]['id'] if msgs else 'NONE')
except: print('JSON_ERROR')
" 2>/dev/null)
echo "Mail ID: $MAIL_ID"

# Reply
$G mail reply $MAIL_ID --body "Reply works"
$G mail list 2>&1 | grep -q "Re:" && echo "PASS: reply" || echo "FAIL"
```

### T1.3: Coordinator Daemon Lifecycle

```bash
G=./target/debug/grove
cd /home/joshf/grove

# Start
$G coordinator start --no-attach
[ -f .overstory/coordinator.pid ] && echo "PASS: pid file" || echo "FAIL"
PID=$(cat .overstory/coordinator.pid)
kill -0 $PID 2>/dev/null && echo "PASS: process alive" || echo "FAIL"

# Status
$G coordinator status 2>&1 | grep -q "Running:.*true" && echo "PASS: status running" || echo "FAIL"

# Send message
$G coordinator send --subject "Test" --body "Hello coordinator"
sleep 2
cat .overstory/logs/coordinator.log | grep -q "Test" && echo "PASS: log received" || echo "FAIL"

# Stop
$G coordinator stop
sleep 1
kill -0 $PID 2>/dev/null && echo "FAIL: still alive" || echo "PASS: stopped"
[ ! -f .overstory/coordinator.pid ] && echo "PASS: pid cleaned" || echo "FAIL: pid file remains"
```

### T1.4: Merge with Content Displacement

```bash
G=./target/debug/grove
cd /home/joshf/grove

# Setup: create two branches that both modify the same area
git checkout -b merge-test-a main
echo "// Added by branch A" >> src/json.rs
git add -A && git commit -m "branch A change" --quiet

git checkout -b merge-test-b main
echo "// Added by branch B" >> src/json.rs  
git add -A && git commit -m "branch B change" --quiet

git checkout main

# Merge A (should be clean)
$G merge --branch merge-test-a 2>&1 | head -3
echo "merge A exit: $?"

# Merge B (should detect displacement)
MERGE_OUT=$($G merge --branch merge-test-b 2>&1)
echo "$MERGE_OUT" | head -5
echo "$MERGE_OUT" | grep -qi "displaced\|conflict" && echo "PASS: displacement detected" || echo "FAIL: no displacement warning"

# Cleanup
git reset --hard HEAD~2 2>/dev/null
git branch -D merge-test-a merge-test-b 2>/dev/null
```

### T1.5: Init → Sling End-to-End (Fresh Project)

```bash
G=/home/joshf/grove/target/debug/grove

rm -rf /tmp/grove-fresh-test && mkdir /tmp/grove-fresh-test && cd /tmp/grove-fresh-test && git init -q
$G init --name fresh-test --yes

# VERIFY: everything init creates
[ -f .overstory/config.yaml ] && echo "PASS: config" || echo "FAIL"
[ -f templates/overlay.md.tmpl ] && echo "PASS: template" || echo "FAIL"
grep -q "fresh-test" .overstory/config.yaml && echo "PASS: project name" || echo "FAIL"

# VERIFY: sling works in the fresh project
$G spec write test-task --body "A test task"
SLING_OUT=$($G sling test-task --capability builder --name test-builder --spec .overstory/specs/test-task.md --files README.md --skip-task-check --no-scout-check 2>&1)
echo "$SLING_OUT"
echo "$SLING_OUT" | grep -qi "launched" && echo "PASS: sling in fresh project" || echo "FAIL"

# VERIFY: status, doctor work
$G status 2>&1 | grep -q "test-builder" && echo "PASS: status" || echo "FAIL"
$G doctor 2>&1 | grep -q "passed" && echo "PASS: doctor" || echo "FAIL"

# Cleanup
$G clean --worktrees --sessions --force
cd /home/joshf/grove
```

---

## TIER 2: OBSERVABILITY — Verify data is correct, not just non-empty

### T2.1: Costs Accuracy

```bash
G=./target/debug/grove
cd /home/joshf/grove

# Compare grove costs with ov costs
GROVE_COST=$($G costs --json 2>&1)
OV_COST=$(ov costs --json 2>&1)

# Schema comparison
echo "grove keys: $(echo $GROVE_COST | python3 -c 'import sys,json; print(sorted(json.load(sys.stdin).keys()))' 2>&1)"
echo "ov keys:    $(echo $OV_COST | python3 -c 'import sys,json; print(sorted(json.load(sys.stdin).keys()))' 2>&1)"

# Do both report same number of sessions?
GROVE_COUNT=$(echo $GROVE_COST | python3 -c 'import sys,json; print(len(json.load(sys.stdin).get("sessions",[])))' 2>/dev/null)
OV_COUNT=$(echo $OV_COST | python3 -c 'import sys,json; print(len(json.load(sys.stdin).get("sessions",[])))' 2>/dev/null)
echo "grove sessions: $GROVE_COUNT, ov sessions: $OV_COUNT"
[ "$GROVE_COUNT" = "$OV_COUNT" ] && echo "PASS: session count match" || echo "FAIL: mismatch"
```

### T2.2: Logs Format Comparison

```bash
G=./target/debug/grove
cd /home/joshf/grove

echo "=== grove logs ==="
$G logs --limit 3 2>&1

echo ""
echo "=== ov logs ==="
ov logs --limit 3 2>&1

# NOTE: Verify manually — grove uses "tool_end", ov uses "tool.end"
# This is a format discrepancy that should be documented or fixed
```

### T2.3: Run List/Show

```bash
G=./target/debug/grove
cd /home/joshf/grove

$G run list 2>&1
# EXPECTED: Should show at least 1 run (we've run multiple phases)
# If "no runs" → BUG: not reading run data correctly

$G run list --json 2>&1 | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'runs: {len(d.get(\"runs\",[]))}'); print(d.keys())" 2>&1
```

### T2.4: Feed vs Events DB

```bash
G=./target/debug/grove
cd /home/joshf/grove

FEED_COUNT=$($G feed --limit 100 2>&1 | grep -c "^[0-9]")
echo "Feed shows $FEED_COUNT events"
# Should be > 0 if events.db has data

$G feed --json --limit 5 2>&1 | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'events: {len(d.get(\"events\",[]))}'); print(sorted(d.keys()))" 2>&1
```

### T2.5: Metrics Accuracy

```bash
G=./target/debug/grove
cd /home/joshf/grove

$G metrics 2>&1
$G metrics --json 2>&1 | python3 -c "
import sys,json
d=json.load(sys.stdin)
print(f'total_sessions: {d.get(\"total_sessions\")}')
print(f'completed: {d.get(\"completed\")}')
print(f'avg_duration: {d.get(\"avg_duration_ms\")}')
" 2>&1
```

### T2.6: Replay Event Ordering

```bash
G=./target/debug/grove
cd /home/joshf/grove

$G replay --limit 10 2>&1
# VERIFY: events are in chronological order (timestamps ascending)
# VERIFY: events span multiple agents
```

### T2.7: Trace Per-Agent

```bash
G=./target/debug/grove
cd /home/joshf/grove

# Get a known agent name from status
AGENT=$($G status --json 2>&1 | python3 -c "
import sys,json
d=json.load(sys.stdin)
if d['agents']: print(d['agents'][0]['agentName'])
else: print('NO_AGENTS')
" 2>/dev/null)

echo "Tracing: $AGENT"
$G trace $AGENT 2>&1 | head -10
# VERIFY: shows events only for this agent
# VERIFY: chronological order
```

---

## TIER 3: TUI — Manual testing in tmux

### T3.1: Overview Renders Correctly

```bash
G=./target/debug/grove
cd /home/joshf/grove

tmux new-session -d -s tui-test "$G dashboard"
sleep 3

# Capture and verify panels exist
OUTPUT=$(tmux capture-pane -t tui-test -p)
echo "$OUTPUT" | grep -q "AGENTS" && echo "PASS: agent panel" || echo "FAIL"
echo "$OUTPUT" | grep -q "FEED" && echo "PASS: feed panel" || echo "FAIL"
echo "$OUTPUT" | grep -q "MAIL" && echo "PASS: mail panel" || echo "FAIL"
echo "$OUTPUT" | grep -q "MERGE" && echo "PASS: merge bar" || echo "FAIL"
echo "$OUTPUT" | grep -q "grove dashboard" && echo "PASS: header" || echo "FAIL"

tmux send-keys -t tui-test 'q'
```

### T3.2: Help Overlay

```bash
tmux new-session -d -s tui-help "$G dashboard"
sleep 2
tmux send-keys -t tui-help '?'
sleep 1

OUTPUT=$(tmux capture-pane -t tui-help -p)
echo "$OUTPUT" | grep -qi "help\|keyboard\|shortcut\|keys" && echo "PASS: help visible" || echo "FAIL: help not rendering"

# Dismiss
tmux send-keys -t tui-help '?'
sleep 1
tmux send-keys -t tui-help 'q'
```

### T3.3: Event Log View

```bash
tmux new-session -d -s tui-events "$G dashboard"
sleep 2
tmux send-keys -t tui-events '2'
sleep 1

OUTPUT=$(tmux capture-pane -t tui-events -p)
EVENT_LINES=$(echo "$OUTPUT" | grep -c "20[0-9][0-9]")
echo "Event lines visible: $EVENT_LINES"
[ "$EVENT_LINES" -gt 5 ] && echo "PASS: events populated" || echo "FAIL: events sparse"

tmux send-keys -t tui-events 'q'
```

### T3.4: Terminal View (requires live agents)

```bash
# Start an agent first
cd /home/joshf/grove
ov sling tui-terminal-test --capability builder --name terminal-test-agent --skip-task-check --no-scout-check --files src/main.rs
sleep 10

tmux new-session -d -s tui-term "$G dashboard"
sleep 2

# Navigate to agent and press t
tmux send-keys -t tui-term 'j'  # select first agent
sleep 1
tmux send-keys -t tui-term 't'  # terminal view
sleep 2

OUTPUT=$(tmux capture-pane -t tui-term -p)
echo "$OUTPUT" | grep -qi "TERMINAL" && echo "PASS: terminal view" || echo "FAIL"
echo "$OUTPUT" | grep -c "." | xargs -I{} echo "Lines with content: {}"

tmux send-keys -t tui-term 'q'
ov clean --worktrees --sessions
```

### T3.5: Split Terminal View

```bash
# Requires 2+ live agents — test during next phase build
# Press 's' from terminal view
# VERIFY: 2x2 grid appears
# VERIFY: each panel shows different agent
# VERIFY: tab cycles focus
```

### T3.6: Mail Reader View

```bash
tmux new-session -d -s tui-mail "$G dashboard"
sleep 2

# Focus mail panel
tmux send-keys -t tui-mail Tab  # move to feed
sleep 0.5
tmux send-keys -t tui-mail Tab  # move to mail
sleep 0.5
tmux send-keys -t tui-mail Enter  # open mail reader
sleep 1

OUTPUT=$(tmux capture-pane -t tui-mail -p)
echo "$OUTPUT" | head -10
# VERIFY: full message body is displayed
# VERIFY: from/to/subject shown

tmux send-keys -t tui-mail 'q'
```

### T3.7: Rich Feed Format

```bash
tmux new-session -d -s tui-feed "$G dashboard"
sleep 3

OUTPUT=$(tmux capture-pane -t tui-feed -p)

# Check for parsed format (not raw JSON)
echo "$OUTPUT" | grep -qE '^\$|✎|⏹|✉' && echo "PASS: rich feed" || echo "WARN: may still show raw events"

# Verify no raw {"command": in the feed panel
echo "$OUTPUT" | grep "FEED" -A 20 | grep -c '{"command"'
# Should be 0 or very low

tmux send-keys -t tui-feed 'q'
```

### T3.8: Status Bar System Info

```bash
tmux new-session -d -s tui-sysinfo "$G dashboard"
sleep 3

OUTPUT=$(tmux capture-pane -t tui-sysinfo -p)
echo "$OUTPUT" | grep -q "load" && echo "PASS: system load" || echo "FAIL"
echo "$OUTPUT" | grep -q "disk\|avail" && echo "PASS: disk info" || echo "FAIL"
echo "$OUTPUT" | grep -q "⎇\|main" && echo "PASS: git branch" || echo "FAIL"

tmux send-keys -t tui-sysinfo 'q'
```

---

## TIER 4: INTEROP — Schema and behavior parity with overstory

### T4.1: JSON Schema Match

```bash
G=./target/debug/grove
cd /home/joshf/grove

for cmd in "status --json" "costs --json"; do
  GROVE_KEYS=$($G $cmd 2>&1 | python3 -c "import sys,json; print(sorted(json.load(sys.stdin).keys()))" 2>&1)
  OV_KEYS=$(ov $cmd 2>&1 | python3 -c "import sys,json; print(sorted(json.load(sys.stdin).keys()))" 2>&1)
  if [ "$GROVE_KEYS" = "$OV_KEYS" ]; then
    echo "PASS: $cmd schema match"
  else
    echo "FAIL: $cmd schema mismatch"
    echo "  grove: $GROVE_KEYS"
    echo "  ov:    $OV_KEYS"
  fi
done
```

### T4.2: Database Compatibility

```bash
cd /home/joshf/grove

# Verify grove and ov read same session count
GROVE_AGENTS=$($G status --json 2>&1 | python3 -c "import sys,json; print(len(json.load(sys.stdin)['agents']))")
OV_AGENTS=$(ov status --json 2>&1 | python3 -c "import sys,json; print(len(json.load(sys.stdin)['agents']))")
echo "grove agents: $GROVE_AGENTS, ov agents: $OV_AGENTS"
[ "$GROVE_AGENTS" = "$OV_AGENTS" ] && echo "PASS: agent count match" || echo "FAIL"

# Verify mail count matches
GROVE_MAIL=$($G status --json 2>&1 | python3 -c "import sys,json; print(json.load(sys.stdin)['unreadMailCount'])")
OV_MAIL=$(ov status --json 2>&1 | python3 -c "import sys,json; print(json.load(sys.stdin)['unreadMailCount'])")
echo "grove unread: $GROVE_MAIL, ov unread: $OV_MAIL"
[ "$GROVE_MAIL" = "$OV_MAIL" ] && echo "PASS: mail count match" || echo "FAIL"
```

---

## TIER 5: ERROR HANDLING

### T5.1: Graceful Failures

```bash
G=./target/debug/grove

# Non-existent project
$G --project /nonexistent status 2>&1 | grep -qi "error\|not found" && echo "PASS: bad project" || echo "FAIL"

# Empty task ID
$G sling "" --capability builder --name x --skip-task-check --no-scout-check 2>&1 | grep -qi "error\|empty\|required" && echo "PASS: empty task" || echo "FAIL"

# Bad capability
$G sling test --capability nonexistent --name x --skip-task-check --no-scout-check 2>&1 | grep -qi "error\|invalid\|unknown" && echo "PASS: bad capability" || echo "FAIL"

# Non-existent agent
$G stop nonexistent-agent 2>&1 | grep -qi "not found" && echo "PASS: bad stop" || echo "FAIL"
$G nudge nonexistent-agent --message "test" 2>&1 | grep -qi "not found\|no active" && echo "PASS: bad nudge" || echo "FAIL"

# Duplicate agent name
cd /home/joshf/grove
$G sling dup-test --capability builder --name dup-agent --skip-task-check --no-scout-check --files src/main.rs 2>&1 | head -1
$G sling dup-test2 --capability builder --name dup-agent --skip-task-check --no-scout-check --files src/main.rs 2>&1 | grep -qi "already\|exists\|duplicate" && echo "PASS: duplicate name" || echo "FAIL"
$G clean --worktrees --sessions --force
```

### T5.2: Empty Database State

```bash
G=/home/joshf/grove/target/debug/grove

# Fresh project with no data
rm -rf /tmp/grove-empty-test && mkdir /tmp/grove-empty-test && cd /tmp/grove-empty-test && git init -q
$G init --name empty-test --yes

# All these should return gracefully, not panic
$G --project /tmp/grove-empty-test status 2>&1 | head -3; echo "exit: $?"
$G --project /tmp/grove-empty-test feed 2>&1 | head -3; echo "exit: $?"
$G --project /tmp/grove-empty-test errors 2>&1 | head -3; echo "exit: $?"
$G --project /tmp/grove-empty-test costs 2>&1 | head -3; echo "exit: $?"
$G --project /tmp/grove-empty-test metrics 2>&1 | head -3; echo "exit: $?"
$G --project /tmp/grove-empty-test logs --limit 5 2>&1 | head -3; echo "exit: $?"
$G --project /tmp/grove-empty-test replay --limit 5 2>&1 | head -3; echo "exit: $?"
$G --project /tmp/grove-empty-test run list 2>&1 | head -3; echo "exit: $?"
$G --project /tmp/grove-empty-test agents 2>&1 | head -3; echo "exit: $?"
$G --project /tmp/grove-empty-test prime 2>&1 | head -3; echo "exit: $?"

cd /home/joshf/grove
```

---

## TIER 6: NEVER-TESTED CRITICAL PATHS

### T6.1: Grove Native Process Spawning (THE BIG ONE)

Grove's whole reason for existing is direct process spawning without tmux. This has NEVER been tested end-to-end.

```bash
G=./target/debug/grove
cd /home/joshf/grove

# This test requires grove to spawn a Claude Code process via process/spawn.rs
# and communicate via stdin/stdout pipes (not tmux)
#
# Currently, grove sling STILL uses tmux for the actual Claude Code session
# because the runtime adapter (runtimes/claude.rs) builds a tmux spawn command.
#
# To truly test grove-native spawning, we need to:
# 1. Verify process/spawn.rs can launch a child process
# 2. Verify process/monitor.rs captures its stdout as NDJSON
# 3. Verify the watchdog can monitor the process health
#
# FOR NOW: verify the code paths exist and unit tests pass
cargo test process:: 2>&1 | grep "test result"
cargo test watchdog:: 2>&1 | grep "test result"

# FUTURE: when grove has its own spawn path that bypasses tmux,
# run a real agent through it and verify the full lifecycle
```

### T6.2: Monitor Daemon

```bash
G=./target/debug/grove
cd /home/joshf/grove

$G monitor start 2>&1
sleep 2
$G monitor status 2>&1
# VERIFY: shows running state

# Spawn an agent, verify monitor picks it up
$G sling monitor-test --capability builder --name monitor-test-agent --skip-task-check --no-scout-check --files src/main.rs
sleep 5
$G monitor status 2>&1
# VERIFY: shows agent being monitored

$G monitor stop 2>&1
$G clean --worktrees --sessions --force
```

### T6.3: Watch Command

```bash
G=./target/debug/grove
cd /home/joshf/grove

# Watch requires live agents — test during next phase build
# Start watch in tmux, spawn agents, verify output updates
tmux new-session -d -s watch-test "$G watch"
# ... spawn agents ...
# VERIFY: watch shows live updates
```

---

## Execution Notes

- **Run Tier 1 before every phase dispatch** — these are the quality gates
- **Run Tier 2-4 at phase conclusion** — per RETRO-014 checklist
- **Run Tier 5 after any error handling changes**
- **Tier 6 tracks when grove gains full native spawning** — revisit after coordinator event loop is complete
- All tests assume `cargo build` has been run and `./target/debug/grove` exists
- Tests that modify git state (merge tests) should be run carefully and cleaned up
