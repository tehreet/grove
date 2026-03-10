# Phase 5 Bugfix: 3 Bugs from E2E Testing

## Bugs

### BUG-1: `grove init` doesn't create templates/overlay.md.tmpl

`grove sling` fails in any grove-initialized project because the overlay template is missing.

**Fix:** Two changes:
1. In `src/commands/init.rs`, create `templates/` directory and write `overlay.md.tmpl` during init. The template content should be embedded at compile time using `include_str!("../../templates/overlay.md.tmpl")` in a constant, then written to disk during init.
2. In `src/agents/overlay.rs` `render_overlay_from_template()`, if the file doesn't exist on disk, fall back to the embedded template constant.

### BUG-2: `grove group add` fails — "Group not found"

After `grove group create mygroup`, `grove group add mygroup issue-1` returns "Group not found."

**Fix:** In `src/commands/group.rs`, the `add` and `status` subcommands look up groups by the auto-generated ID (e.g., `group-0a43bfbf`), but the user passes the group name (e.g., `mygroup`). Fix the lookup to search by name first, then by ID. The groups.json has both the ID as the key and "name" as a field — search the values for matching name if the key doesn't match.

### BUG-3: `grove group status` fails — same root cause as BUG-2

Same fix as BUG-2. Both `add` and `status` need name-based lookup.

## File Scope

- `src/commands/init.rs` — embed and write overlay template
- `src/agents/overlay.rs` — fallback to embedded template
- `src/commands/group.rs` — fix name-based group lookup

## Verification Commands

```bash
G=/home/joshf/grove/target/debug/grove

# BUG-1: init creates template, sling works in fresh project
rm -rf /tmp/grove-bugfix-test && mkdir /tmp/grove-bugfix-test && cd /tmp/grove-bugfix-test && git init -q
$G init --name bugfix-test --yes 2>&1 | tail -1
[ -f templates/overlay.md.tmpl ] && echo "BUG-1a PASS: template created" || echo "BUG-1a FAIL"
$G sling test-task --capability builder --name test-agent --skip-task-check --no-scout-check --files src/main.rs 2>&1 | head -1
# Should say "Agent launched" not "Failed to read overlay template"
$G sling test-task --capability builder --name test-agent --skip-task-check --no-scout-check --files src/main.rs 2>&1 | grep -q "launched\|Agent" && echo "BUG-1b PASS: sling works" || echo "BUG-1b FAIL"

# BUG-2: group add works by name
cd /home/joshf/grove
$G group create verify-group 2>&1
$G group add verify-group test-issue-1 2>&1
$G group add verify-group test-issue-1 2>&1 | grep -qi "added\|success\|already" && echo "BUG-2 PASS" || echo "BUG-2 FAIL"

# BUG-3: group status works by name
$G group status verify-group 2>&1 | head -3
$G group status verify-group 2>&1 | grep -qi "verify-group\|active\|issues" && echo "BUG-3 PASS" || echo "BUG-3 FAIL"

# All tests still pass
cargo build && cargo test && echo "ALL PASS"
```
