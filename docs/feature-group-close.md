# Feature: grove group close

## Context

`grove group` manages task groups (collections of issues). It has create, status, add, remove, list subcommands. It's missing a `close` subcommand that manually closes a group.

Groups are stored in `.overstory/groups.json` as a JSON array of TaskGroup objects.

## What To Build

Add a `close` subcommand to `grove group`:

```
grove group close <group-id>
```

Behavior:
- Look up the group by ID or name in `.overstory/groups.json`
- Set the group's `status` field to `"closed"` 
- Set `completed_at` to the current ISO 8601 timestamp
- Save the updated groups back to the file
- Print a success message: `✓ Group closed <name> (<id>)`
- With `--json`, output: `{"command":"group-close","success":true,"groupId":"...","name":"..."}`
- If the group doesn't exist, return an error: `Group "<id>" not found`
- If the group is already closed, return an error: `Group "<id>" is already closed`

## Files To Modify

1. `src/commands/group.rs` — Add `pub fn execute_close(group_id, json, project_override)` function.
   Follow the pattern of the existing `execute_remove` function for structure.

2. `src/main.rs` — Add to `GroupSubcommand` enum:
   ```rust
   /// Close a task group
   Close(GroupCloseArgs),
   ```
   
   Add the args struct:
   ```rust
   #[derive(Parser, Debug)]
   struct GroupCloseArgs {
       /// Group ID or name
       group_id: String,
       #[arg(long)]
       json: bool,
   }
   ```
   
   Add the match arm in the `Commands::Group` handler:
   ```rust
   GroupSubcommand::Close(a) => commands::group::execute_close(
       &a.group_id,
       a.json || json,
       project,
   ),
   ```

## Verification Commands

Run these in sequence to verify the feature works:

```bash
G=./target/debug/grove

# Build
cargo build

# Tests pass
cargo test

# Create a test group
$G group create test-close-group issue-1 issue-2

# List shows it as active
$G group list | grep test-close-group

# Close it
$G group close test-close-group

# List shows it as closed
$G group list | grep -i "closed"

# Closing again should error
$G group close test-close-group 2>&1 | grep "already closed"

# Closing nonexistent should error
$G group close nonexistent-group 2>&1 | grep "not found"

# JSON output works
$G group create test-close-json issue-3
$G group close test-close-json --json | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['success']==True"

# Clippy clean
cargo clippy -- -D warnings
```

## Acceptance Criteria

1. `grove group close <id>` closes a group and prints success
2. Already-closed groups return an error
3. Nonexistent groups return an error
4. `--json` output matches the format above
5. `cargo build && cargo test && cargo clippy -- -D warnings` all pass
6. The match arm is wired in main.rs (not a stub)
