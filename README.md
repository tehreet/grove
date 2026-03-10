# grove

[![CI](https://github.com/tehreet/grove/actions/workflows/ci.yml/badge.svg)](https://github.com/tehreet/grove/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Multi-agent orchestration for AI coding agents. A Rust rebuild of [overstory](https://github.com/jayminwest/overstory) — single binary, no tmux, multi-runtime.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/tehreet/grove/main/install.sh | sh
```

Or from source:
```sh
cargo install --git https://github.com/tehreet/grove
```

## Quick Start

```sh
cd my-project && grove init

# Write a task spec
grove spec write login-page --body "Build the login page with OAuth support"

# Dispatch a builder agent (Claude Code)
grove sling login-page --capability builder --name login-builder \
  --spec .overstory/specs/login-page.md --files src/auth/

# Or use Codex instead
grove sling login-page --runtime codex --capability builder --name login-builder \
  --spec .overstory/specs/login-page.md --files src/auth/

# Monitor
grove status
grove dashboard

# Merge when done
grove merge --branch overstory/login-builder/login-page
```

## Why Grove

| | overstory (TypeScript) | grove (Rust) |
|---|---|---|
| **Agent spawning** | tmux sessions | Direct child processes |
| **Runtime** | Bun required | Single binary, no deps |
| **Runtimes** | Claude, Codex, Gemini, Copilot, Pi, Sapling, OpenCode | Same (4 real + 3 stubs) |
| **Merge safety** | Silent content displacement | `ContentDisplaced` typed handling |
| **Nudges** | tmux send-keys (fragile) | Mail-based (reliable) |
| **Distribution** | `npm install` | `curl \| sh` |

Grove reads and writes the same `.overstory/` databases as overstory. You can use both on the same project.

## Commands

35 commands. Run `grove --help` for the full list.

```
grove init            Initialize .overstory/
grove sling           Spawn an agent in a worktree
grove status          Show running agents
grove dashboard       TUI dashboard (7 views)
grove mail            Agent mail system
grove coordinator     Coordinator daemon
grove merge           Merge agent branches
grove monitor         PID lifecycle daemon
grove costs           Token/cost analysis
grove doctor          System health check
grove completions     Shell completions
grove upgrade         Self-update
```

## Runtime Adapters

Grove supports multiple AI coding agents via runtime adapters:

| Runtime | Instruction File | Spawn Command |
|---------|-----------------|---------------|
| Claude Code | `.claude/CLAUDE.md` | `claude -p` |
| Codex (OpenAI) | `AGENTS.md` | `codex exec --dangerously-bypass-approvals-and-sandbox` |
| Gemini (Google) | `GEMINI.md` | `gemini -p --yolo` |
| Copilot (GitHub) | `.github/copilot-instructions.md` | `copilot -p --allow-all-tools` |

Per-capability routing lets you use different runtimes for different agent types:
```yaml
# .overstory/config.yaml
runtime:
  default: claude
  capabilities:
    builder: codex
    lead: claude
```

## Architecture

See [docs/architecture.md](docs/architecture.md).

- **No tmux.** Agents are child processes. PIDs in sessions.db. Monitor daemon detects death via `/proc/<pid>`.
- **Coordinator is Rust, not LLM.** Event loop daemon. LLM only for one-shot task decomposition.
- **Typed merge outcomes.** `ContentDisplaced` forces handling of silently dropped content.
- **WAL mode SQLite.** Concurrent access from multiple agent processes.

## Shell Completions

```sh
grove completions bash > ~/.local/share/bash-completion/completions/grove
grove completions zsh > ~/.zfunc/_grove
grove completions fish > ~/.config/fish/completions/grove.fish
```

## License

MIT
