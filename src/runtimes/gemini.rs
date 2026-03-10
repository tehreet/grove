//! Gemini (Google) runtime adapter.
//!
//! Spawns agents via `gemini -p "prompt" --yolo` for headless mode.
//! Instructions delivered via GEMINI.md (Gemini CLI's native convention).
//! --yolo flag enables auto-approval of tool calls.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::{AgentRuntime, HooksDef, ReadyPhase, ReadyState, SpawnOpts};

pub struct GeminiRuntime;

impl AgentRuntime for GeminiRuntime {
    fn id(&self) -> &str {
        "gemini"
    }

    fn instruction_path(&self) -> &str {
        "GEMINI.md"
    }

    fn is_headless(&self) -> bool {
        true
    }

    fn build_headless_command(&self, opts: &SpawnOpts) -> Vec<String> {
        vec![
            "gemini".to_string(),
            "-p".to_string(),
            format!(
                "Read {} for your task assignment and begin immediately.",
                opts.instruction_path
            ),
            "--yolo".to_string(),
        ]
    }

    fn build_interactive_command(&self, opts: &SpawnOpts) -> String {
        format!("gemini -m {}", opts.model)
    }

    fn deploy_config(
        &self,
        worktree: &Path,
        overlay_content: &str,
        _hooks: &HooksDef,
    ) -> Result<(), String> {
        // Gemini reads GEMINI.md in the worktree root
        // No hooks deployment — Gemini uses --yolo for auto-approval
        if !overlay_content.is_empty() {
            let gemini_path = worktree.join("GEMINI.md");
            fs::write(&gemini_path, overlay_content)
                .map_err(|e| format!("Failed to write GEMINI.md: {e}"))?;
        }
        Ok(())
    }

    fn detect_ready(&self, _pane_content: &str) -> ReadyState {
        ReadyState {
            phase: ReadyPhase::Ready,
            detail: Some("Gemini ready".to_string()),
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

    fn make_opts() -> SpawnOpts {
        SpawnOpts {
            model: "gemini-2.5-pro".to_string(),
            cwd: "/tmp".to_string(),
            permission_mode: "bypass".to_string(),
            allowed_tools: vec!["Read".to_string(), "Write".to_string()],
            instruction_path: "GEMINI.md".to_string(),
        }
    }

    #[test]
    fn test_gemini_id() {
        assert_eq!(GeminiRuntime.id(), "gemini");
    }

    #[test]
    fn test_gemini_instruction_path() {
        assert_eq!(GeminiRuntime.instruction_path(), "GEMINI.md");
    }

    #[test]
    fn test_gemini_headless_command() {
        let cmd = GeminiRuntime.build_headless_command(&make_opts());
        assert_eq!(cmd[0], "gemini");
        assert_eq!(cmd[1], "-p");
        assert!(cmd[2].contains("GEMINI.md"));
        assert_eq!(cmd[3], "--yolo");
    }

    #[test]
    fn test_gemini_deploy_writes_gemini_md() {
        let dir = TempDir::new().unwrap();
        let hooks = HooksDef {
            agent_name: "test".to_string(),
            capability: "builder".to_string(),
            worktree_path: dir.path().to_string_lossy().to_string(),
            quality_gates: None,
        };
        GeminiRuntime
            .deploy_config(dir.path(), "# Gemini overlay", &hooks)
            .unwrap();

        let gemini_md = dir.path().join("GEMINI.md");
        assert!(gemini_md.exists());
        assert_eq!(
            std::fs::read_to_string(gemini_md).unwrap(),
            "# Gemini overlay"
        );
        assert!(!dir.path().join(".claude").exists());
    }
}
