# grove

[![CI](https://github.com/tehreet/grove/actions/workflows/ci.yml/badge.svg)](https://github.com/tehreet/grove/actions/workflows/ci.yml)
[![Latest Release](https://img.shields.io/github/v/release/tehreet/grove)](https://github.com/tehreet/grove/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Multi-agent orchestration for AI coding agents. A Rust rebuild of [overstory](https://github.com/jayminwest/overstory) — faster, smaller, single-binary.

## Install

**One-liner:**
```sh
curl -fsSL https://raw.githubusercontent.com/tehreet/grove/main/install.sh | sh
```

**From source:**
```sh
cargo install --git https://github.com/tehreet/grove
```

**Download binary:**
Download the latest release for your platform from [GitHub Releases](https://github.com/tehreet/grove/releases/latest):
- `grove-linux-amd64` / `grove-linux-arm64`
- `grove-darwin-amd64` / `grove-darwin-arm64`
- `grove-windows-amd64.exe`

## Quick Start

```sh
# Initialize a project
grove init

# Dispatch a task to an agent
grove sling --agent builder --task "Build the login page"

# Check agent status
grove status

# Open the TUI dashboard
grove dashboard
```

## Why Rust?

Grove is a ground-up rewrite of overstory, fixing architectural problems in the original:

| | overstory (TypeScript) | grove (Rust) |
|---|---|---|
| **Runtime** | Bun required | Single binary, no deps |
| **Database** | bun:sqlite (sync) | rusqlite bundled SQLite |
| **Concurrency** | Single-threaded coordinator | Tokio async event loop |
| **Merge safety** | Silent content displacement | `ContentDisplaced` forces handling |
| **Distribution** | `bun install` | `curl \| sh` |

Grove interoperates with overstory — it reads and writes the same `.overstory/` databases, so you can migrate incrementally.

## Commands

```
grove init            Initialize .overstory/ in the current directory
grove sling           Dispatch a task to an agent worktree
grove status          Show running agents and system state
grove dashboard       TUI dashboard (live view)
grove mail            Send/receive agent mail
grove coordinator     Start/stop the coordinator daemon
grove agents          List agent definitions
grove worktree        Manage git worktrees
grove merge           Merge an agent's branch
grove completions     Generate shell completions
grove update          Refresh managed files from built-in defaults
grove upgrade         Self-update grove binary
grove doctor          Check system dependencies
```

See `grove --help` for the full command list.

## Shell Completions

```sh
# Bash
grove completions bash > ~/.local/share/bash-completion/completions/grove

# Zsh
grove completions zsh > ~/.zfunc/_grove

# Fish
grove completions fish > ~/.config/fish/completions/grove.fish
```

## Architecture

See [docs/architecture.md](docs/architecture.md) for the full design rationale.

Key decisions:
- **No tmux for spawning.** Agents are child processes with stdin/stdout pipes.
- **Coordinator is a Rust event loop, not an LLM.** LLM called only for task decomposition.
- **Typed merge outcomes.** `MergeOutcome::ContentDisplaced` fixes overstory's silent content-drop bug.
- **WAL mode SQLite.** Multiple processes read/write concurrently without locking.

## License

MIT
