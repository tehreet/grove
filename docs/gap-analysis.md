# Grove vs Overstory: Full Feature Gap Analysis

**Date:** 2026-03-10 (pre-Phase 9A/9B — tmux since eliminated, adapters since added)
**Grove:** 27,452 lines, 85 Rust files, 454 tests
**Overstory:** ~150,000+ lines (including tests), 85+ TypeScript files

---

## 1. RUNTIME ADAPTERS — THE BIGGEST GAP

### Overstory: 7 adapters
| Runtime | ID | Instruction File | Headless | Interactive | RPC |
|---------|-----|------------------|----------|-------------|-----|
| Claude Code | claude | .claude/CLAUDE.md | `claude --print -p` | `claude --model X` | No |
| Codex (OpenAI) | codex | AGENTS.md | `codex exec --full-auto` | `codex --full-auto` | No |
| Copilot | copilot | .github/copilot-instructions.md | `copilot -p` | `copilot --model X` | No |
| Gemini | gemini | GEMINI.md | `gemini -p --yolo` | `gemini -m X` | No |
| OpenCode | opencode | (varies) | `opencode --prompt --format json` | (interactive) | No |
| Pi | pi | .claude/CLAUDE.md | `pi --print` | `pi --model X` | Yes (JSON-RPC) |
| Sapling | sapling | SAPLING.md | `sp print` | `sp run --model X --json` | Yes |

### Grove: 1 adapter
| Runtime | ID | Instruction File | Headless | Interactive |
|---------|-----|------------------|----------|-------------|
| Claude Code | claude | .claude/CLAUDE.md | Yes | Yes |

### Missing: codex, copilot, gemini, opencode, pi, sapling

### Runtime Trait Gap

Overstory's `AgentRuntime` interface has methods grove's trait doesn't:

| Method | Overstory | Grove | Gap |
|--------|-----------|-------|-----|
| `id` | ✅ | ✅ | — |
| `instructionPath` | ✅ | ✅ | — |
| `buildSpawnCommand` (interactive) | ✅ | ✅ `build_interactive_command` | — |
| `buildPrintCommand` (one-shot AI call) | ✅ | ❌ | **MISSING** — used by merge resolver + watchdog triage |
| `deployConfig` | ✅ | ✅ | — |
| `detectReady` | ✅ | ✅ | — |
| `buildEnv` | ✅ | ✅ | — |
| `parseTranscript` | ✅ | ❌ | **MISSING** — extracts token counts from session NDJSON |
| `getTranscriptDir` | ✅ | ❌ | **MISSING** — locates transcript files per runtime |
| `requiresBeaconVerification` | ✅ | ❌ | **MISSING** — Claude needs beacon resend, Pi doesn't |
| `connect` (RPC) | ✅ (Pi, Sapling) | ❌ | **MISSING** — direct RPC bypasses tmux for mail/nudge |
| `headless` (property) | ✅ | ✅ `is_headless()` | — |
| `buildDirectSpawn` | ✅ | ✅ `build_headless_command` | — |
| `parseEvents` (NDJSON stream) | ✅ | ❌ | **MISSING** — parses stdout into typed AgentEvents |

---

## 2. SUBSYSTEMS — WHAT GROVE DOESN'T HAVE

### 2a. Beads (`src/beads/`)
Overstory has a `beads` (bd) CLI client for the os-eco task tracking system. Used alongside `seeds` (sd) for issue management. Grove has seeds integration but no beads.

**Impact:** Low — beads is an alternative tracker backend. Seeds works.

### 2b. Eval System (`src/eval/`)
Full A/B evaluation framework: define scenarios, run assertions, measure directive effectiveness. 4 files: assertions.ts, runner.ts, scenarios.ts, types.ts. Grove has `eval` as a hidden stub.

**Impact:** Medium — needed for measuring agent quality, not for basic operation.

### 2c. Insights (`src/insights/`)
Session insight analyzer — post-mortem analysis of agent sessions. Extracts patterns from transcripts.

**Impact:** Low — nice to have, not blocking.

### 2d. Mulch Integration (`src/mulch/`)
Deep integration with the mulch (ml) expertise system: directive graduation, domain expertise, prime format. Grove calls `mulch prime` as a shell command but doesn't have native integration.

**Impact:** Medium — mulch directives improve agent quality over time.

### 2e. Tracker Factory (`src/tracker/`)
Unified tracker abstraction supporting both seeds and beads backends. Factory pattern creates the right client based on config.

**Impact:** Low — grove hardcodes seeds.

### 2f. Connections (`src/runtimes/connections.ts`)
RPC connection tracking for headless agents (Pi, Sapling). Manages stdin/stdout pipes for live communication without tmux.

**Impact:** Medium — needed for Pi/Sapling RPC runtimes.

### 2g. Pi Guards (`src/runtimes/pi-guards.ts`)
Security guard extensions for Pi runtime — enforces file scope, quality gates, tool restrictions via Pi's extension system.

**Impact:** Only needed if Pi runtime is added.

---

## 3. COMMAND FEATURE GAPS

### Commands that exist in both but differ:

| Command | Overstory Feature | Grove Status |
|---------|------------------|-------------|
| `sling --runtime X` | Routes to any of 7 runtimes | Only routes to claude |
| `sling` headless | buildDirectSpawn + spawnHeadlessAgent | ✅ Works (Phase 8.5) |
| `init --skip-mulch` | Skips mulch bootstrap | Not implemented |
| `init --skip-seeds` | Skips seeds bootstrap | Not implemented |
| `costs --live` | Real-time token streaming | ✅ Has this |
| `dashboard` | ANSI + Go TUI | ✅ ratatui TUI (better) |
| `eval` | Full A/B framework | Hidden stub |
| `supervisor` | Deprecated but functional | Prints deprecation |
| `merge` AI resolve | Calls runtime.buildPrintCommand for AI conflict resolution | Grove has the resolver but no AI call |
| `logs` format | `tool.end` (dot notation) | ✅ Fixed to match |
| `completions` | 942-line custom completions.ts | ✅ clap_complete (better) |

### Commands grove has that overstory doesn't:
- `grove coordinator` — native Rust daemon (overstory's coordinator is an LLM session)
- `grove monitor` — dedicated PID watchdog daemon
- `grove upgrade` — self-update from GitHub releases
- `grove doctor` — more extensive than ov's

---

## 4. ARCHITECTURAL DIFFERENCES (INTENTIONAL)

| Feature | Overstory | Grove | Why Different |
|---------|-----------|-------|--------------|
| Agent spawning | tmux sessions | Direct child processes (headless) | Eliminates tmux dependency |
| Coordinator | LLM session in tmux | Rust event loop daemon | Deterministic, no LLM needed for orchestration |
| Database | bun:sqlite (sync) | rusqlite with WAL mode | Better concurrency |
| Merge resolver | 4-tier with AI assist | 4-tier with ContentDisplaced type | Typed displacement detection |
| TUI | Go bubbletea binary | Rust ratatui (built-in) | Single binary, no Go dependency |
| Distribution | npm package | Single static binary | Zero runtime dependencies |
| Config | In-memory reload | Load from YAML | Same behavior, different implementation |
| Watchdog | Tier 0/1/2 in one process | Separate monitor daemon | More Unix-y, daemon per concern |

---

## 5. WHAT'S NEEDED TO USE GROVE WITH CODEX RIGHT NOW

To run `grove sling task --headless --runtime codex`:

1. **Codex runtime adapter** (`src/runtimes/codex.rs`):
   - `id()` → "codex"
   - `instruction_path()` → "AGENTS.md"
   - `build_headless_command()` → `["codex", "exec", "--full-auto", "--ephemeral", prompt]`
   - `build_interactive_command()` → `"codex --full-auto 'Read AGENTS.md...'"`
   - `deploy_config()` → write overlay to `AGENTS.md` (not `.claude/CLAUDE.md`)
   - `detect_ready()` → always ready
   - `build_env()` → pass through OPENAI_API_KEY

2. **Register in registry** (`src/runtimes/registry.rs`):
   - Add `"codex" => Box::new(CodexRuntime)`

3. **Overlay writes to correct path**:
   - Currently hardcoded to `.claude/CLAUDE.md` in some places
   - Codex expects `AGENTS.md` in worktree root
   - The overlay writer already uses `runtime.instruction_path()` — should work

Similarly for gemini (GEMINI.md), copilot (.github/copilot-instructions.md), etc.

---

## 6. PRIORITY RANKING FOR CLOSING GAPS

### P0 — Need before grove can replace overstory
1. **Codex runtime adapter** — you have a working Codex login right now
2. **Gemini runtime adapter** — you have a Gemini API key
3. **Registry wiring** — register new adapters

### P1 — Need for production quality
4. **parseTranscript** method on trait — extract token usage from session transcripts
5. **buildPrintCommand** method on trait — AI-assisted merge conflict resolution
6. **parseEvents** method on trait — NDJSON stdout parsing for live event feed
7. **Auto-merge on agent completion** — coordinator should queue completed branches

### P2 — Nice to have
8. Copilot adapter
9. OpenCode adapter
10. Pi adapter + Pi guards + RPC connections
11. Sapling adapter + RPC
12. Eval system
13. Insights analyzer
14. Beads tracker
15. Mulch native integration

### P3 — Future
16. requiresBeaconVerification per runtime
17. Full tracker factory abstraction
18. Cross-runtime transcript normalization

---

## 7. HONEST ASSESSMENT

Grove has ~90% feature parity with overstory on the COMMAND level — every command works. But it has ~15% feature parity on the RUNTIME level — only Claude Code is supported. This means:

- **If you only use Claude Code:** Grove is production-ready. Ship it.
- **If you want to use Codex, Gemini, or any other runtime:** Grove is missing the adapters. You cannot `grove sling --runtime codex` today.

The runtime adapters are the #1 gap. Each one is ~200-400 lines of Rust (matching the ~200-400 line TypeScript adapter in overstory). Adding Codex and Gemini would take ~600 lines total and immediately unlock two alternative runtimes.

Everything else (eval, insights, beads, mulch deep integration) is secondary — the runtimes are what make grove usable with different AI providers.
