# Grove Integration Testing Plan

## Why This Matters

We have 305 unit tests but they only test functions in isolation against in-memory SQLite. We haven't verified that grove's commands work end-to-end, that they interoperate with overstory, or that the data they write is actually correct. Phase 4 (coordinator) will depend on every piece below working perfectly. If any of these fail, the coordinator will silently break.

---

## Test 1: Full Sling Lifecycle (CRITICAL)

The most important test. If sling doesn't work, nothing works.

```
1. grove spec write test-lifecycle --body "Test task"
2. grove sling test-lifecycle --capability builder --name lifecycle-agent --spec .overstory/specs/test-lifecycle.md --files src/main.rs --skip-task-check --no-scout-check
3. VERIFY: .overstory/worktrees/lifecycle-agent/ exists
4. VERIFY: .overstory/worktrees/lifecycle-agent/.claude/CLAUDE.md is NOT empty
5. VERIFY: CLAUDE.md contains "lifecycle-agent" and "test-lifecycle" and "src/main.rs"
6. VERIFY: .overstory/worktrees/lifecycle-agent/.claude/settings.local.json has hooks
7. VERIFY: git branch contains "overstory/lifecycle-agent/test-lifecycle"
8. VERIFY: grove status shows lifecycle-agent as working/booting
9. VERIFY: grove status --json has agent with correct fields
10. grove log session-start --agent lifecycle-agent
11. VERIFY: grove status shows lifecycle-agent state = working
12. grove log session-end --agent lifecycle-agent --exit-code 0
13. VERIFY: grove status shows lifecycle-agent state = completed
14. grove clean --worktrees --sessions
15. VERIFY: worktree directory gone, session gone from status
```

## Test 2: Overlay Content Verification (KNOWN BUG)

The overlay bug we found — deploy_config overwrites CLAUDE.md with empty string.

```
1. grove sling overlay-test --capability builder --name overlay-test --spec .overstory/specs/test-lifecycle.md --files src/types.rs,src/config.rs --skip-task-check --no-scout-check
2. Read .overstory/worktrees/overlay-test/.claude/CLAUDE.md
3. VERIFY contains: "overlay-test" (agent name)
4. VERIFY contains: "overlay-test" (task id in assignment block)
5. VERIFY contains: "src/types.rs" (file scope)
6. VERIFY contains: "src/config.rs" (file scope)
7. VERIFY contains: spec path
8. VERIFY contains: "builder" (capability)
9. VERIFY contains: quality gate commands (cargo test, etc.)
10. VERIFY does NOT contain any un-replaced "{{VARIABLE}}" template markers
```

## Test 3: Mail Round-Trip

```
1. grove mail send --to agent-a --subject "Test 1" --body "Hello" --from agent-b
2. grove mail send --to agent-a --subject "Test 2" --body "World" --from agent-c --priority high
3. grove mail check --agent agent-a
4. VERIFY: shows 2 unread messages
5. grove mail list --to agent-a
6. VERIFY: both messages appear with correct from/subject/priority
7. grove mail read <id-from-step-1>
8. VERIFY: shows full message, marks as read
9. grove mail check --agent agent-a
10. VERIFY: shows 1 unread (only Test 2)
11. grove mail reply <id-from-step-1> --body "Reply text"
12. VERIFY: grove mail list shows reply with "Re: Test 1" subject
13. grove mail check --agent agent-b (reply should go to agent-b)
14. VERIFY: agent-b has 1 unread
```

## Test 4: Interop with Overstory (CRITICAL)

Grove and overstory must read/write the same databases.

```
1. In grove repo: grove mail send --to interop-test --subject "From grove" --body "Written by Rust"
2. In grove repo: ov mail list (overstory reads what grove wrote)
3. VERIFY: overstory sees the message with correct fields
4. In grove repo: ov mail send --to interop-test2 --subject "From overstory" --body "Written by TS"  
5. In grove repo: grove mail list (grove reads what overstory wrote)
6. VERIFY: grove sees the overstory message
7. grove status --json > /tmp/grove-status.json
8. ov status --json > /tmp/ov-status.json
9. DIFF the JSON keys and structure (values may differ but schema must match)
```

## Test 5: Config Loading Edge Cases

```
1. In /tmp, create minimal .overstory/config.yaml with just project.name
2. grove --project /tmp status
3. VERIFY: loads with defaults for all missing fields, doesn't crash
4. Create config with invalid quality gate (empty command)
5. VERIFY: grove doctor warns about it
6. Create config.local.yaml with override
7. VERIFY: local override is applied
```

## Test 6: Clean Command (Destructive)

```
1. Create 2 test worktrees via sling
2. grove status → shows 2 agents
3. grove clean --worktrees --sessions
4. VERIFY: .overstory/worktrees/ is empty
5. VERIFY: git branches for those agents are deleted
6. VERIFY: grove status shows 0 agents
7. VERIFY: sessions.db is clean
```

## Test 7: Init in Fresh Directory

```
1. mkdir /tmp/grove-init-test && cd /tmp/grove-init-test && git init
2. grove init --name fresh-project --yes
3. VERIFY: .overstory/config.yaml exists with project.name = fresh-project
4. VERIFY: .overstory/agent-manifest.json exists
5. VERIFY: config.yaml has correct project.root (absolute path)
6. grove --project /tmp/grove-init-test doctor
7. VERIFY: basic checks pass (git, config, etc.)
8. grove --project /tmp/grove-init-test status
9. VERIFY: runs without error, shows empty state
```

## Test 8: Error Handling

```
1. grove sling "" → VERIFY: clear error about empty task ID
2. grove sling task --capability nonexistent → VERIFY: error about unknown capability
3. grove sling task --spec /nonexistent/path → VERIFY: error about spec file not found
4. grove stop nonexistent-agent → VERIFY: error about agent not found
5. grove mail read nonexistent-id → VERIFY: error about message not found
6. grove --project /nonexistent status → VERIFY: error about project not found
7. grove sling already-taken --name X, then grove sling another --name X → VERIFY: error about duplicate name
```

## Test 9: JSON Output Compatibility

```
For each implemented command, compare --json output schema:
1. grove status --json vs ov status --json → same top-level keys
2. grove mail list --json (if supported) vs ov mail list --json
3. grove costs --json vs ov costs --json
4. Verify all JSON includes "success" and "command" fields
```

## Test 10: Merge Resolver Content Displacement Detection

```
1. Create a test git repo with a file
2. Create branch-a: add "Section A" at line 10
3. Create branch-b: add "Section B" at line 10  
4. Merge branch-a (clean)
5. Merge branch-b (conflict)
6. grove merge should detect displaced content from branch-a
7. VERIFY: MergeOutcome is ContentDisplaced, not silent success
```

---

## Priority Order

1. **Test 1 (Sling Lifecycle)** — if this fails, Phase 4 is dead
2. **Test 2 (Overlay)** — known bug, must verify fix
3. **Test 4 (Interop)** — if grove can't interop with overstory, the whole project is pointless
4. **Test 3 (Mail)** — coordinator depends entirely on mail working
5. **Test 8 (Error Handling)** — bad errors = confused agents = wasted money
6. **Test 6 (Clean)** — broken clean = accumulated garbage = disk full
7. **Tests 5,7,9,10** — important but less critical for Phase 4
