# Grove — Overstory Rebuilt in Rust

## What This Is

A Rust rebuild of [overstory](https://github.com/jayminwest/overstory), a multi-agent orchestration system for AI coding agents. Not a line-by-line port — a rebuild that preserves the interface and data contracts while fixing architectural problems that the TypeScript implementation can't solve.

Overstory spawns worker agents in git worktrees, coordinates them through SQLite mail, and merges their work with tiered conflict resolution. It has 35 CLI commands, 7 runtime adapters, 36k lines of TypeScript source, and 57k lines of tests. It works — but it has real problems that stem from implementation choices, not design choices.

Grove keeps everything that works (the agent model, the mail system, the merge tiers, the overlay template system, the mulch expertise layer) and rebuilds the parts that don't (tmux-based spawning, process-blind orchestration, the coordinator-as-LLM pattern, the merge resolver's silent failures).

Ships as a single static binary. No Bun, no npm, no Node ecosystem. Just `grove`.

---

## Why Not Just Port

A 1:1 port would give us a faster binary with the same bugs. The issues, PRs, and battle scars from the TypeScript codebase reveal structural problems that deserve structural solutions. Here are the six biggest, with detailed analysis of how they work today and how Grove addresses them.

---

### Problem 1: The tmux spawning model is fundamentally fragile

**GitHub issues:** #85 (switch to headless), #87 (detectReady false-positive), #93 (dedicated tmux socket), #73 (WSL2 pane failures), #83 (Windows support), #86 (fish shell breaks export syntax)

**How it works today:**

When overstory spawns an agent, it goes through this sequence in `src/commands/sling.ts` (1,130 lines) and `src/worktree/tmux.ts` (582 lines):

```
1. Create git worktree + branch
2. Write overlay (CLAUDE.md) to worktree
3. Create tmux session: tmux new-session -d -s <name> "claude --model X"
4. Poll tmux capture-pane every 500ms for up to 30 seconds
   - Parse pane content looking for "❯" (prompt char) and "bypass permissions"
   - If "trust this folder" dialog detected, send Enter to dismiss
   - If pane dies during polling, abort
5. Sleep 1 second (buffer for input handler attachment)
6. Build beacon string (agent name, task ID, startup instructions)
7. Send beacon via tmux send-keys
8. Send follow-up Enters with escalating delays: 1s, 2s, 3s, 5s
   - Claude Code's TUI sometimes consumes Enter during late init
9. Verify beacon was received (up to 5 attempts):
   - Capture pane, check if still at welcome screen
   - If yes, resend entire beacon + Enter
   - Sleep 2s between attempts
10. Hope the agent starts working
```

That's 11 steps, 6 sleeps (totaling 11+ seconds of dead waiting), screen scraping via regex, and a 5-attempt retry loop for a fundamental operation: "start this program with this input."

The code literally has comments like:
- `"Claude Code's TUI sometimes consumes the Enter keystroke during late initialization, swallowing the beacon text entirely (overstory-3271)"`
- `"Pi's TUI idle and processing states are indistinguishable via detectReady"`
- `"An Enter on an empty input line is harmless"` (justifying blind Enter-spamming)

Each runtime adapter must implement `detectReady(paneContent: string): ReadyState` which parses terminal screen captures to determine if the TUI has rendered. The Claude adapter looks for the `❯` character and "bypass permissions" text. The Pi adapter can't distinguish idle from processing. The Copilot adapter has a different dialog flow. Every runtime is a special case of screen scraping.

**Why it's fragile:**

- Shell configuration leaks in. Fish shell users get broken `export` syntax (#86). Oh-my-zsh with slow plugins delays TUI startup past the 30s timeout. Personal tmux config (themes, plugins) interferes with programmatic session creation (#93).
- The beacon verification loop is a heuristic. It works ~95% of the time. The other 5% produces "zombie" agents that started the TUI but never received their task.
- Windows doesn't have tmux (#83). WSL2 has race conditions with tmux panes (#73). This locks out a significant user base.
- Every new runtime adapter requires implementing screen-scraping detection logic specific to that runtime's TUI. The Sapling adapter (707 lines) spent significant effort on this.

**How Grove does it:**

Grove doesn't use tmux for agent management. Agents are child processes spawned with Rust's `tokio::process::Command` with captured stdin/stdout/stderr pipes.

```rust
// Spawn agent as a child process
let mut child = Command::new(&runtime.binary)
    .args(&runtime.build_args(model, worktree_path))
    .current_dir(worktree_path)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()?;

// Send the task prompt directly via stdin
let stdin = child.stdin.take().unwrap();
stdin.write_all(beacon.as_bytes()).await?;
stdin.write_all(b"\n").await?;
drop(stdin); // Close stdin — signals end of input for -p mode runtimes

// Monitor stdout for NDJSON events
let stdout = BufReader::new(child.stdout.take().unwrap());
let event_stream = parse_ndjson_events(stdout);

// Process lifecycle is tracked directly
let pid = child.id().unwrap();
session_store.register(agent_name, pid, ...);
```

No screen scraping. No sleep escalation. No beacon verification. No tmux dependency. The prompt goes directly to stdin. The agent's events come directly from stdout. Process lifecycle (alive/dead/exit code) is tracked through the OS process handle, not tmux session polling.

**For runtimes that require interactive TUI mode** (some runtimes don't support `-p`/headless), Grove uses a pseudo-terminal (PTY) via the `portable-pty` crate instead of tmux. This gives direct stdin/stdout access with proper terminal emulation — no shell config leakage, no tmux server, cross-platform.

**What this enables:**

- Windows support natively (no WSL, no tmux). `Command::new()` works everywhere.
- Fish, zsh, bash — doesn't matter. No shell is involved in the spawn path.
- Agent startup takes <1 second instead of 11+ seconds.
- Zero zombie agents from swallowed beacons. If stdin write succeeds, the agent received the prompt.
- New runtime adapters don't need screen-scraping logic. They just need `build_args()` and `parse_events()`.

---

### Problem 2: The merge resolver silently drops content

**GitHub issue:** #89 (critical priority) — "auto-resolve merge tier silently drops content from prior merge"

**How it works today:**

The 4-tier resolver in `src/merge/resolver.ts` (1,125 lines) works like this:

```
Tier 1 (clean-merge):  git merge --no-edit
Tier 2 (auto-resolve): parse conflict markers, keep incoming changes
Tier 3 (ai-resolve):   send conflicting hunks to Claude --print for resolution
Tier 4 (reimagine):    abort merge, ask agent to reimplement from scratch
```

The problem is Tier 2. When two agents modify the same region of a file, `git merge` produces conflict markers. The auto-resolve tier parses these markers with regex and keeps the "incoming" (current agent's) changes, discarding the "ours" (previously merged agent's) changes.

The return type tells you nothing about what was lost:

```typescript
// src/types.ts
export interface MergeResult {
    entry: MergeEntry;
    success: boolean;        // true — looks fine!
    tier: ResolutionTier;    // "auto-resolve" — ok
    conflictFiles: string[]; // ["README.md"] — had conflicts
    errorMessage: string | null; // null — no error!
}
```

`success: true`, `errorMessage: null`. The merge "succeeded." But Agent A's "Prerequisites" section was silently replaced by Agent B's "Quick Start" section. No warning, no diff output, no record of what was dropped. The content is permanently gone.

**How Grove does it:**

The merge result is an enum that makes dropped content impossible to ignore:

```rust
pub enum MergeOutcome {
    /// Clean fast-forward or no-conflict merge.
    Clean {
        merged_files: Vec<String>,
    },
    
    /// Conflicts existed but were resolved automatically or by AI.
    Resolved {
        tier: ResolutionTier,
        resolutions: Vec<ConflictResolution>,
    },
    
    /// Resolution succeeded but content from a prior merge was displaced.
    /// The merge is committed but the caller MUST surface this to the user.
    ContentDisplaced {
        tier: ResolutionTier,
        displaced: Vec<DisplacedHunk>,
        resolutions: Vec<ConflictResolution>,
    },
    
    /// Merge failed at all tiers.
    Failed {
        attempted_tiers: Vec<ResolutionTier>,
        reason: String,
    },
}

pub struct DisplacedHunk {
    pub file: String,
    pub original_agent: String,
    pub displaced_lines: usize,
    pub content_preview: String, // First 200 chars of what was lost
}
```

You literally cannot match on `MergeOutcome` without handling `ContentDisplaced`. The Rust compiler forces it. The coordinator sees the displaced content and can either escalate to AI-resolve (which can intelligently combine both additions) or flag it for human review.

The detection works by comparing the file content before and after auto-resolve: if lines from the "ours" side that aren't conflict markers disappeared, it's displacement, not resolution. This is a diff operation, not a regex parse.

Additionally, Grove uses the `git2` crate (libgit2 bindings) for merge operations instead of shelling out to the git CLI. This gives access to the merge index — individual conflict entries with their base/ours/theirs content — instead of parsing conflict markers from file content. The merge is done at the data structure level, not the text level.

---

### Problem 3: The coordinator is an LLM when it should be an event loop

**GitHub issues:** #97 (coordinator doesn't push state files), #105 (coordinator can't handle sub-directory agents), #106 (workflow profiles: delivery vs co-creation)

**How it works today:**

The coordinator is a Claude Code session running inside tmux (`src/commands/coordinator.ts`, 1,361 lines). It's a real LLM conversation that:

- Reads its mail via hooks (`ov mail check --inject`)
- Decides what to do based on its agent definition (`agents/coordinator.md`)
- Spawns leads/builders by calling `ov sling` through bash
- Monitors progress by reading status updates from mail
- Decides when to merge, when to nudge stalled agents, when to escalate

This means the coordinator:
- **Costs money to idle.** It's a Claude session burning tokens just waiting for mail. Every 30s the hook fires, checks mail, and Claude thinks about whether to do something. At Opus pricing, an idle coordinator costs ~$2/hour.
- **Can forget context.** After compaction, the coordinator may lose track of which agents it spawned, which merges are pending, and what the overall objective was.
- **Is non-deterministic.** The same mail arriving at the same time might produce different orchestration decisions depending on context window state.
- **Can't handle complexity it hasn't seen.** Issue #105: the coordinator can't manage agents across sub-directories because it would need to understand multi-repo topology — a reasoning challenge, not an engineering one.

**How Grove does it:**

The coordinator is a native Rust event loop. No LLM. It polls the SQLite databases (sessions, mail, events, merge queue) on a 1-second tick and dispatches actions based on state transitions:

```rust
pub struct Coordinator {
    config: CoordinatorConfig,
    session_store: SessionStore,
    mail_store: MailStore,
    merge_queue: MergeQueue,
    event_store: EventStore,
    planner: Planner, // This is where the LLM is used — one-shot planning only
}

impl Coordinator {
    pub async fn run(&mut self) -> Result<()> {
        loop {
            // 1. Check for completed agents
            let completed = self.session_store.get_completed_since(self.last_check);
            for agent in completed {
                self.handle_agent_completion(agent).await?;
            }

            // 2. Check mail
            let messages = self.mail_store.get_unread("coordinator");
            for msg in messages {
                self.handle_message(msg).await?;
            }

            // 3. Check merge queue
            let ready = self.merge_queue.get_pending();
            for entry in ready {
                self.handle_merge(entry).await?;
            }

            // 4. Check health (stale/zombie detection)
            self.health_check().await?;

            // 5. Check exit triggers
            if self.should_exit() {
                break;
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        Ok(())
    }
}
```

The LLM is called **only** for task decomposition — when a new objective arrives, the planner (one-shot Claude API call, same as overstory's existing print-mode pattern) breaks it into tasks. Everything else — spawning, monitoring, merging, health checks, exit triggers — is deterministic Rust code.

**Benefits:**
- Zero cost to idle. The event loop polls SQLite. No tokens consumed.
- No context loss. State is in the database, not in an LLM conversation window.
- Deterministic. Same state → same actions. Every time.
- Handles sub-directories (#105) trivially — it's just file paths in the session store.
- Workflow profiles (#106) become a config enum that changes the event loop behavior:

```rust
pub enum WorkflowProfile {
    /// Autonomous execution. Auto-approve merges, no checkpoints.
    Delivery,
    /// Interactive co-creation. Pause at milestones, wait for human input.
    CoCreation { checkpoint_interval: Duration },
}
```

In delivery mode, the coordinator auto-merges when quality gates pass. In co-creation mode, it pauses and shows a TUI dialog asking the human to review before proceeding. This is a match arm in the event loop, not a prompt engineering challenge.

---

### Problem 4: The system is blind to agent internals

**GitHub issues:** #67 (rate limit fallback), #81 (cost-tier routing), PR #98 (rate limit detection + runtime swap), PR #99 (show runtime in dashboard)

**How it works today:**

Overstory is a process-level orchestrator. It spawns an agent and then observes it through two channels:

1. **Mail** — agents send structured messages (`worker_done`, `error`, `status`). This is voluntary. If the agent crashes before sending mail, overstory learns nothing.
2. **Health polling** — the watchdog checks if the tmux session exists and has recent activity. It can detect "stalled" (no activity for 5min) and "zombie" (no activity for 10min), but can't distinguish "thinking hard on a complex problem" from "stuck in a retry loop hitting rate limits."

There's no real-time token tracking, no rate limit detection, no awareness of which model the agent is actually using (vs which was requested), no cost monitoring during execution. The `costs` command reads transcript files after the fact.

The event tailer (`src/events/tailer.ts`) was added to poll NDJSON from headless agents' stdout.log files, but it's a bolted-on afterthought — it reads log files by byte offset, misses events during file rotation, and doesn't feed back into orchestration decisions.

**How Grove does it:**

Because Grove owns the agent's stdout pipe directly (no tmux intermediary), it can parse NDJSON events in real-time:

```rust
pub struct AgentMonitor {
    child: Child,
    event_tx: mpsc::Sender<AgentEvent>,
    token_budget: TokenBudget,
}

impl AgentMonitor {
    pub async fn monitor(&mut self) -> Result<AgentOutcome> {
        let stdout = BufReader::new(self.child.stdout.take().unwrap());
        let mut lines = stdout.lines();

        while let Some(line) = lines.next_line().await? {
            if let Ok(event) = serde_json::from_str::<AgentEvent>(&line) {
                // Real-time token tracking
                if let AgentEvent::TokenUsage { input, output, model, .. } = &event {
                    self.token_budget.record(*input, *output, model);
                    
                    // Cost circuit breaker
                    if self.token_budget.exceeds_limit() {
                        self.child.kill().await?;
                        return Ok(AgentOutcome::BudgetExceeded(self.token_budget.snapshot()));
                    }
                }

                // Rate limit detection
                if let AgentEvent::ApiError { status: 429, retry_after, .. } = &event {
                    self.event_tx.send(AgentEvent::RateLimited {
                        agent: self.name.clone(),
                        retry_after: *retry_after,
                    }).await?;
                }

                // Forward to event store
                self.event_tx.send(event).await?;
            }
        }

        let status = self.child.wait().await?;
        Ok(AgentOutcome::Exited(status.code()))
    }
}
```

This enables:

- **Cost circuit breaker** — if an agent exceeds 2x its budget estimate, kill it and report. No runaway costs.
- **Rate limit awareness** — when an agent hits 429, the coordinator knows immediately and can pause other agents hitting the same provider, or swap to a different model tier.
- **Real-time cost tracking** — the TUI shows live cost per agent, updated every few seconds, not after the fact.
- **Cost-tier routing** (#81) — the coordinator assigns model tiers per capability from config:

```yaml
models:
  tier1: anthropic/claude-haiku-4-5     # scout, monitor
  tier2: anthropic/claude-sonnet-4-6    # builder, lead
  tier3: anthropic/claude-opus-4-6      # reviewer, coordinator planning

capability_tiers:
  scout: 1
  builder: 2
  reviewer: 3
  lead: 2
  monitor: 1
```

- **Model fallback** (#67) — when tier2 hits rate limits, automatically retry with tier1 at reduced capability rather than killing the agent.

---

### Problem 5: No Windows support, tmux as hard dependency

**GitHub issues:** #83 (Windows via mprocs/psmux), #79 (SessionBackend abstraction), #73 (WSL2 race conditions)

**How it works today:**

Every process management operation goes through tmux:

```typescript
// src/worktree/tmux.ts — 582 lines of tmux-specific code
export async function createTmuxSession(name: string, command: string): Promise<void>
export async function sendKeys(name: string, keys: string): Promise<void>
export async function capturePaneContent(name: string): Promise<string | null>
export async function killSession(name: string, gracePeriodMs?: number): Promise<void>
export async function isSessionAlive(name: string): Promise<boolean>
export async function waitForTuiReady(name: string, ...): Promise<boolean>
export async function listSessions(): Promise<TmuxSession[]>
```

Issue #79 proposed a `SessionBackend` abstraction to support psmux/mprocs on Windows. It was closed because the refactoring burden was too high — tmux assumptions are baked into sling.ts, watchdog, dashboard, inspector, and multiple test files.

**How Grove does it:**

There's no session backend abstraction because there's no session manager. Grove uses OS-level process management:

```rust
pub trait ProcessManager {
    fn spawn(&self, config: SpawnConfig) -> Result<ManagedProcess>;
    fn kill(&self, pid: u32, signal: Signal) -> Result<()>;
    fn is_alive(&self, pid: u32) -> bool;
    fn wait(&self, pid: u32) -> impl Future<Output = Result<ExitStatus>>;
}

// Platform-specific implementations
#[cfg(unix)]
pub struct UnixProcessManager;

#[cfg(windows)]
pub struct WindowsProcessManager;
```

On Unix, `spawn` uses `tokio::process::Command`. On Windows, same thing — `Command::new()` is cross-platform. Signal handling differs (`SIGTERM` vs `TerminateProcess`), but that's a two-line `#[cfg]` branch, not a 582-line abstraction layer.

For the TUI-required runtimes (Claude Code interactive mode, Pi interactive mode), Grove uses the `portable-pty` crate which provides pseudo-terminal support on all platforms — Unix PTYs, Windows ConPTY. This replaces the entire tmux interaction model with direct terminal emulation.

tmux is still **supported** as an optional backend for users who want to attach to agent sessions for debugging (`grove attach <agent-name>`), but it's not in the critical spawn path.

---

### Problem 6: Distribution friction

**Not an issue — it's the permanent background complaint.**

Installing overstory today:

```bash
# Prerequisites
curl -fsSL https://bun.sh/install | bash  # Install Bun
# Ensure git is installed
# Ensure tmux is installed (apt install tmux / brew install tmux)
# Ensure at least one agent runtime is installed

# Install overstory
bun install -g @os-eco/overstory-cli

# Verify
ov doctor
```

That's 4 prerequisites and a global npm-style install. On a fresh machine, this takes 5-10 minutes. On CI, it requires explicit setup steps.

Installing Grove:

```bash
curl -fsSL https://grove.sh/install | sh
grove doctor
```

One command. The install script detects your platform (linux/amd64, darwin/arm64, etc.), downloads the correct binary from GitHub releases, and puts it in your PATH. The binary includes everything — CLI, TUI, SQLite (compiled in via `rusqlite` bundled feature), config parser, template engine. The only external dependencies are git and an agent runtime.

---

## What Stays the Same

These things are correct in overstory and Grove preserves them exactly:

- **SQLite for everything.** WAL mode, 5s busy timeout, concurrent access from multiple agents. The 5 database schemas are identical.
- **Agent definitions as markdown.** The `agents/*.md` files are language-agnostic templates. They work the same in both systems.
- **Overlay template system.** `{{VARIABLE}}` replacement in `templates/overlay.md.tmpl`. Same template, same variables.
- **Mail-based coordination.** SQLite mail with types (status, result, error, worker_done, merge_ready) and priorities. Same protocol.
- **4-tier merge escalation.** Clean → auto-resolve → AI-resolve → reimagine. Same tiers (but auto-resolve no longer drops content silently).
- **Mulch expertise layer.** Mulch client for record/query/prime. Same JSONL format. Directive graduation works the same.
- **Quality gates.** Commands defined in config.yaml, run before merge.
- **CLI interface.** Same 35 commands, same flags, same `--json` output format. `ov` and `grove` are interchangeable on the same `.overstory/` directory.
- **Worktree isolation.** Agents work in git worktrees on separate branches. Same pattern.

---

## Rust Crate Structure

```
grove/
├── Cargo.toml
├── build.rs                     # Embed version, agent defs, template
├── agents/                      # Identical .md files from overstory
├── templates/
│   └── overlay.md.tmpl
├── src/
│   ├── main.rs                  # Entry point, clap CLI
│   ├── types.rs                 # All shared types (72 types, serde-derives)
│   ├── config.rs                # YAML config loader + validation
│   ├── errors.rs                # thiserror error types
│   │
│   ├── db/                      # Database layer (rusqlite, bundled)
│   │   ├── mod.rs
│   │   ├── connection.rs        # WAL mode, busy timeout, shared opener
│   │   ├── sessions.rs
│   │   ├── mail.rs
│   │   ├── events.rs
│   │   ├── metrics.rs
│   │   └── merge_queue.rs
│   │
│   ├── process/                 # Process management (replaces worktree/tmux.ts)
│   │   ├── mod.rs
│   │   ├── spawn.rs             # Child process spawning (stdin/stdout pipes)
│   │   ├── monitor.rs           # NDJSON event parsing, token tracking, budget
│   │   ├── pty.rs               # PTY for interactive runtimes (portable-pty)
│   │   ├── lifecycle.rs         # PID tracking, signal handling, cleanup
│   │   └── platform.rs          # #[cfg(unix)] / #[cfg(windows)] specifics
│   │
│   ├── coordinator/             # Native event loop (replaces LLM coordinator)
│   │   ├── mod.rs
│   │   ├── event_loop.rs        # Poll databases, dispatch actions
│   │   ├── planner.rs           # One-shot LLM call for task decomposition
│   │   ├── profiles.rs          # Delivery vs CoCreation workflow modes
│   │   └── triggers.rs          # Exit trigger evaluation
│   │
│   ├── merge/                   # Merge subsystem (with content-displacement detection)
│   │   ├── mod.rs
│   │   ├── resolver.rs          # 4-tier with DisplacedHunk detection
│   │   ├── queue.rs             # FIFO merge queue
│   │   └── libgit2.rs           # git2 crate merge operations
│   │
│   ├── commands/                # CLI commands (same interface as overstory)
│   │   ├── [35 command modules]
│   │
│   ├── agents/                  # Overlay generation, manifests, guard rules
│   ├── runtimes/                # 7+ runtime adapters
│   ├── watchdog/                # Health monitoring
│   ├── worktree/                # Git worktree management (no tmux!)
│   ├── logging/                 # Terminal output, brand palette
│   ├── mulch/                   # Expertise system + directives
│   ├── tracker/                 # Seeds/beads adapters
│   ├── eval/                    # Eval system
│   │
│   └── tui/                     # ratatui dashboard
│       ├── app.rs
│       ├── views/
│       │   ├── overview.rs
│       │   ├── agent_detail.rs
│       │   ├── event_log.rs
│       │   └── help.rs
│       ├── widgets/
│       │   ├── agent_table.rs
│       │   ├── feed.rs
│       │   ├── mail_list.rs
│       │   ├── status_bar.rs
│       │   └── sparkline.rs
│       └── theme.rs
│
└── tests/
    ├── fixtures/                # SQLite databases with known state
    └── integration/             # JSON output compatibility tests
```

### Rust Dependencies

```toml
[dependencies]
clap = { version = "4", features = ["derive", "env"] }
rusqlite = { version = "0.31", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
chrono = { version = "0.4", features = ["serde"] }
thiserror = "1"
anyhow = "1"
tokio = { version = "1", features = ["full"] }
crossterm = "0.27"
ratatui = "0.27"
git2 = "0.19"                    # libgit2 bindings for merge operations
portable-pty = "0.8"             # Cross-platform PTY for interactive runtimes
reqwest = { version = "0.12", features = ["json"] }  # Claude API for planner + AI-resolve
colored = "2"
which = "6"
uuid = { version = "1", features = ["v4"] }
rand = "0.8"
similar = "2"                    # Diff library for content-displacement detection
glob = "0.3"

[dev-dependencies]
tempfile = "3"
assert_cmd = "2"
predicates = "3"
```

---

## Build Phases

### Phase 0: Foundation
Types, config, errors, database layer, CLI skeleton, logging. `grove --help` works.

### Phase 1: Read-Only Commands (parallel)
`status`, `mail list/check`, `costs`, `doctor`. Same `--json` output as overstory.

### Phase 2: Write Commands (parallel)
`mail send/reply`, `clean`, `stop`, `merge` (with content-displacement detection), `init`, `hooks`, `spec`.

### Phase 3: Process Management + Spawning
`sling` with direct process spawning (no tmux). PTY support for interactive runtimes. Agent monitoring with NDJSON parsing. This is where Grove diverges most from overstory.

### Phase 4: Native Coordinator
Event loop coordinator with one-shot LLM planner. Workflow profiles. Exit triggers. Cost-tier routing.

### Phase 5: Observability (parallel)
`inspect`, `trace`, `feed`, `replay`, `errors`, `logs`, `metrics`. Plus the ratatui TUI dashboard.

### Phase 6: Polish
Shell completions, self-upgrade, cross-compilation CI, install script.

---

## Integration Testing

The contract: identical `--json` output for identical database state.

```bash
# For every --json command:
ov status --json > /tmp/ov.json
grove status --json > /tmp/grove.json
diff /tmp/ov.json /tmp/grove.json  # Must be empty
```

For write commands, compare database state after identical operations. For the merge resolver, verify that `ContentDisplaced` is returned in cases where overstory returns `success: true` with silent data loss.

A project initialized with `ov init` must work with `grove` and vice versa — they share the `.overstory/` directory.

---

## Success Criteria

1. Single static binary, <20MB, <10ms startup, <10MB RSS
2. All 35 commands with identical `--json` output
3. Windows support without WSL (process management + TUI)
4. Merge resolver never silently drops content (#89 fixed by design)
5. Coordinator costs zero tokens when idle (event loop, not LLM)
6. Real-time token tracking and cost circuit breaker per agent
7. Agent spawn takes <1s (no tmux, no sleep escalation)
8. ratatui TUI with keyboard navigation, drill-down views, live data
9. Cross-compiles: linux/{amd64,arm64}, darwin/{amd64,arm64}, windows/amd64
10. CI passes: `cargo test`, `cargo clippy`, `cargo fmt --check`, integration tests
