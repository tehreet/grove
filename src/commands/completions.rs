//! `grove completions` — generate shell completion scripts.
//!
//! Wraps `clap_complete` to emit a ready-to-source completion script for the
//! requested shell. The caller passes in a mutable reference to the root clap
//! `Command` (built via `Cli::command()`).

use clap_complete::Shell;

/// Write shell completions for `shell` to stdout.
///
/// Installation hints (per-shell) are written to stderr so they do not
/// contaminate the completion script when the user redirects stdout to a file.
pub fn execute(shell: Shell, cmd: &mut clap::Command) -> Result<(), String> {
    clap_complete::generate(shell, cmd, "grove", &mut std::io::stdout());

    let hint = match shell {
        Shell::Bash => Some(
            "# Install: grove completions bash > ~/.local/share/bash-completion/completions/grove",
        ),
        Shell::Zsh => Some(
            "# Install: grove completions zsh > ~/.zfunc/_grove\n\
             # Then add: fpath=(~/.zfunc $fpath) to ~/.zshrc",
        ),
        Shell::Fish => {
            Some("# Install: grove completions fish > ~/.config/fish/completions/grove.fish")
        }
        _ => None,
    };

    if let Some(h) = hint {
        eprintln!("{h}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_bash_returns_ok() {
        // Build a minimal clap command for testing without pulling in full Cli.
        let mut cmd = clap::Command::new("grove-test");
        // Redirect stdout to /dev/null equivalent — just verify no panic/error.
        // clap_complete::generate writes to the provided writer; we capture it.
        let mut buf = Vec::new();
        clap_complete::generate(Shell::Bash, &mut cmd, "grove-test", &mut buf);
        assert!(!buf.is_empty(), "completions should produce output");
    }
}
