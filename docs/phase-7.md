# Phase 7: Distribution & Polish

## Context

Phases 0-6 are complete. Grove has full feature parity with overstory, a working TUI dashboard, and a native coordinator event loop. This phase makes grove distributable — shell completions, self-update, cross-compilation, and release packaging.

## Deliverables

### 1. `grove completions` — Shell completions

Reference: `reference/completions.ts` (942 lines — but clap generates these natively)

```
grove completions <shell>
```

Where shell is: bash, zsh, fish, powershell, elvish

Use clap's built-in `clap_complete` crate to generate completions. This is trivial in Rust:

```rust
use clap::CommandFactory;
use clap_complete::{generate, Shell};

fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "grove", &mut std::io::stdout());
}
```

Add `clap_complete` to Cargo.toml dependencies.

Print instructions for installation:
- bash: `grove completions bash > ~/.local/share/bash-completion/completions/grove`
- zsh: `grove completions zsh > ~/.zfunc/_grove`
- fish: `grove completions fish > ~/.config/fish/completions/grove.fish`

### 2. `grove update` — Refresh managed files

Reference: `reference/update.ts`

```
grove update [--agents] [--manifest] [--hooks] [--dry-run] [--json]
```

Refreshes `.overstory/` managed files from grove's embedded defaults:
- `--agents`: Overwrite agent definition files in `.overstory/agent-defs/` with grove's built-in versions (from `agents/` directory, embedded at compile time via `include_str!` or `build.rs`)
- `--manifest`: Refresh `agent-manifest.json` with current defaults
- `--hooks`: Refresh `hooks.json`
- `--dry-run`: Show what would change without writing
- No flags: refresh all

Compare existing files with embedded defaults. Only overwrite if different. Report what changed.

### 3. `grove upgrade` — Self-update

Reference: `reference/upgrade.ts`

```
grove upgrade [--check] [--all] [--json]
```

- `--check`: Query GitHub releases API for latest grove version, compare with current, print result
- No `--check`: Download latest release binary for current platform, replace current binary
- `--all`: Also upgrade os-eco ecosystem tools (run `mulch upgrade`, `sd upgrade`, `cn upgrade`)

Implementation:
1. Query `https://api.github.com/repos/tehreet/grove/releases/latest` via reqwest
2. Parse version from tag name
3. Compare with current version (embedded at compile time)
4. If newer: download binary asset for current platform (`grove-{os}-{arch}`)
5. Replace current binary (write to temp file, then rename — atomic swap)
6. Print "Upgraded grove from v0.1.0 to v0.2.0"

For `--all`, shell out to each ecosystem tool's upgrade command.

### 4. `build.rs` — Compile-time embedding

Create/update `build.rs` to embed:
- Git version tag (or fallback to Cargo.toml version)
- Git commit hash (short)
- Build timestamp
- Agent definition files (for `grove update --agents`)
- Overlay template (for `grove sling` to find even without the templates/ directory)

```rust
// build.rs
fn main() {
    // Version from git tag
    let version = std::process::Command::new("git")
        .args(["describe", "--tags", "--always"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());

    println!("cargo:rustc-env=GROVE_VERSION={}", version);
    println!("cargo:rustc-env=GROVE_BUILD_TIME={}", chrono::Utc::now().to_rfc3339());
}
```

### 5. Cross-compilation CI — GitHub Actions

Create `.github/workflows/release.yml`:

**Targets:**
- `x86_64-unknown-linux-gnu` (linux/amd64)
- `aarch64-unknown-linux-gnu` (linux/arm64)
- `x86_64-apple-darwin` (darwin/amd64)
- `aarch64-apple-darwin` (darwin/arm64)
- `x86_64-pc-windows-msvc` (windows/amd64)

**Workflow:**
1. Trigger on tag push (`v*`)
2. Matrix build across all 5 targets
3. Use `cross` crate for cross-compilation (or native runners for macOS/Windows)
4. Strip binaries (`strip` on Linux/macOS)
5. Create GitHub Release with all 5 binaries as assets
6. Name assets: `grove-linux-amd64`, `grove-linux-arm64`, `grove-darwin-amd64`, `grove-darwin-arm64`, `grove-windows-amd64.exe`

Also create `.github/workflows/ci.yml`:
1. Trigger on push to main and PRs
2. Run `cargo build`, `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`

### 6. Install script

Create `install.sh` at repo root:

```bash
#!/bin/sh
set -e
REPO="tehreet/grove"
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
  x86_64|amd64) ARCH="amd64" ;;
  aarch64|arm64) ARCH="arm64" ;;
  *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac
ASSET="grove-${OS}-${ARCH}"
URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"
echo "Downloading grove for ${OS}/${ARCH}..."
curl -fsSL -o /tmp/grove "$URL"
chmod +x /tmp/grove
sudo mv /tmp/grove /usr/local/bin/grove
echo "grove installed to /usr/local/bin/grove"
grove --version
```

### 7. README.md

Rewrite the repo README with:
- What grove is (one paragraph)
- Install instructions (curl | sh, cargo install, download binary)
- Quick start (grove init, grove sling, grove status, grove dashboard)
- Comparison with overstory (why Rust, what's different)
- Link to architecture.md for full details
- Badge: CI status, latest release, license

## File Scope

New files:
- `src/commands/completions.rs`
- `src/commands/update_cmd.rs`
- `src/commands/upgrade_cmd.rs`
- `build.rs`
- `.github/workflows/release.yml`
- `.github/workflows/ci.yml`
- `install.sh`
- `README.md` (rewrite)

Modified files:
- `Cargo.toml` — add `clap_complete` dependency
- `src/commands/mod.rs` — register new modules
- `src/main.rs` — wire completions, update, upgrade, use embedded version from build.rs

## Quality Gates

- `cargo build` — clean
- `cargo test` — all tests pass
- `cargo clippy -- -D warnings`
- `cargo fmt --check`
- `grove completions bash > /dev/null` — generates without error
- `grove update --dry-run` — shows what would change without writing
- `grove upgrade --check` — queries GitHub (may fail if no release exists yet, that's OK)

## Verification Commands

```bash
G=./target/debug/grove

# 1. Completions
$G completions bash > /tmp/grove-completions.bash
[ -s /tmp/grove-completions.bash ] && echo "PASS: bash completions generated" || echo "FAIL"
$G completions zsh > /tmp/grove-completions.zsh
[ -s /tmp/grove-completions.zsh ] && echo "PASS: zsh completions generated" || echo "FAIL"
$G completions fish > /tmp/grove-completions.fish
[ -s /tmp/grove-completions.fish ] && echo "PASS: fish completions generated" || echo "FAIL"

# 2. Update
$G update --dry-run 2>&1 | head -5
$G update --dry-run 2>&1 | grep -qi "would\|change\|up to date" && echo "PASS: update dry-run works" || echo "FAIL"

# 3. Upgrade
$G upgrade --check 2>&1 | head -3
# May show "no releases found" or "up to date" — both are fine

# 4. Version includes git info
$G --version 2>&1
$G --version 2>&1 | grep -q "grove" && echo "PASS: version shows" || echo "FAIL"

# 5. No stubs remaining
for cmd in "completions bash" update upgrade; do
  result=$($G $cmd 2>&1 | head -1)
  echo "$result" | grep -q "not yet implemented" && echo "FAIL: grove $cmd still stub" || echo "OK: grove $cmd"
done

# 6. CI files exist
[ -f .github/workflows/ci.yml ] && echo "PASS: CI workflow" || echo "FAIL"
[ -f .github/workflows/release.yml ] && echo "PASS: release workflow" || echo "FAIL"
[ -f install.sh ] && echo "PASS: install script" || echo "FAIL"

# 7. Install script is valid
bash -n install.sh && echo "PASS: install.sh syntax valid" || echo "FAIL"

# Quality gates
cargo build && echo "BUILD PASS"
cargo test && echo "TEST PASS"
cargo clippy -- -D warnings && echo "CLIPPY PASS"
cargo fmt --check && echo "FMT PASS"
```

## Acceptance Criteria

1. `grove completions bash/zsh/fish` generates valid shell completions
2. `grove update` refreshes managed files from embedded defaults
3. `grove upgrade --check` queries GitHub for latest version
4. `grove --version` shows version with git commit info
5. CI workflow runs tests on push to main
6. Release workflow builds binaries for 5 platforms on tag push
7. `install.sh` downloads and installs the correct binary for the current platform
8. README.md has install instructions, quick start, and architecture link
9. **Zero `not_yet_implemented` stubs remain in the entire codebase**
10. All tests pass, all clippy warnings clean, code formatted
11. Binary size under 20MB (release profile with LTO and strip)

## Post-Phase 7: Release Checklist

After Phase 7 is complete:
1. Tag: `git tag v0.1.0 && git push --tags`
2. GitHub Actions builds release binaries
3. Test install script: `curl -fsSL https://raw.githubusercontent.com/tehreet/grove/main/install.sh | sh`
4. Verify: `grove --version && grove doctor && grove init --name test --yes`
5. Announce
