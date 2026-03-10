//! Copilot (GitHub) runtime adapter.
//!
//! Spawns agents via `copilot -p "prompt" --allow-all-tools` for headless mode.
//! Instructions delivered via .github/copilot-instructions.md.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::{AgentRuntime, HooksDef, ReadyPhase, ReadyState, SpawnOpts};

pub struct CopilotRuntime;

impl AgentRuntime for CopilotRuntime {
    fn id(&self) -> &str {
        "copilot"
    }

    fn instruction_path(&self) -> &str {
        ".github/copilot-instructions.md"
    }

    fn is_headless(&self) -> bool {
        true
    }

    fn build_headless_command(&self, opts: &SpawnOpts) -> Vec<String> {
        vec![
            "copilot".to_string(),
            "-p".to_string(),
            format!(
                "Read {} for your task assignment and begin immediately.",
                opts.instruction_path
            ),
            "--allow-all-tools".to_string(),
        ]
    }

    fn build_interactive_command(&self, opts: &SpawnOpts) -> String {
        format!("copilot --model {}", opts.model)
    }

    fn deploy_config(
        &self,
        worktree: &Path,
        overlay_content: &str,
        _hooks: &HooksDef,
    ) -> Result<(), String> {
        if !overlay_content.is_empty() {
            let github_dir = worktree.join(".github");
            fs::create_dir_all(&github_dir)
                .map_err(|e| format!("Failed to create .github dir: {e}"))?;
            let path = github_dir.join("copilot-instructions.md");
            fs::write(&path, overlay_content)
                .map_err(|e| format!("Failed to write copilot-instructions.md: {e}"))?;
        }
        Ok(())
    }

    fn detect_ready(&self, _pane_content: &str) -> ReadyState {
        ReadyState {
            phase: ReadyPhase::Ready,
            detail: Some("Copilot ready".to_string()),
        }
    }

    fn build_env(&self, model: &crate::types::ResolvedModel) -> HashMap<String, String> {
        model.env.clone().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_copilot_id() {
        assert_eq!(CopilotRuntime.id(), "copilot");
    }

    #[test]
    fn test_copilot_instruction_path() {
        assert_eq!(CopilotRuntime.instruction_path(), ".github/copilot-instructions.md");
    }

    #[test]
    fn test_copilot_deploy_creates_github_dir() {
        let dir = TempDir::new().unwrap();
        let hooks = HooksDef {
            agent_name: "test".to_string(),
            capability: "builder".to_string(),
            worktree_path: dir.path().to_string_lossy().to_string(),
            quality_gates: None,
        };
        CopilotRuntime
            .deploy_config(dir.path(), "# Copilot overlay", &hooks)
            .unwrap();

        let path = dir.path().join(".github/copilot-instructions.md");
        assert!(path.exists());
    }
}
