//! Codex (OpenAI) runtime adapter.
//!
//! Spawns agents via `codex exec --full-auto --ephemeral` for headless mode.
//! Instructions delivered via AGENTS.md (Codex's native convention).
//! Security enforced via Codex's OS-level sandbox, not hooks.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::{AgentRuntime, HooksDef, ReadyPhase, ReadyState, SpawnOpts};

/// Anthropic model aliases that Codex CLI doesn't accept as --model values.
/// Models that Codex CLI doesn't accept as --model values.
/// When these appear, omit --model and let Codex use its own default.
fn should_omit_model(model: &str) -> bool {
    // Anthropic manifest aliases
    let aliases = ["sonnet", "opus", "haiku"];
    if aliases.contains(&model) {
        return true;
    }
    // Any claude-* model is not a valid Codex model
    if model.starts_with("claude") {
        return true;
    }
    false
}

pub struct CodexRuntime;

impl AgentRuntime for CodexRuntime {
    fn id(&self) -> &str {
        "codex"
    }

    fn instruction_path(&self) -> &str {
        "AGENTS.md"
    }

    fn is_headless(&self) -> bool {
        true
    }

    fn build_headless_command(&self, opts: &SpawnOpts) -> Vec<String> {
        let mut cmd = vec![
            "codex".to_string(),
            "exec".to_string(),
            "--full-auto".to_string(),
            "--ephemeral".to_string(),
        ];

        // Only add --model if it's not a manifest alias (sonnet/opus/haiku)
        // — let Codex use its own configured default for those
        if !should_omit_model(&opts.model) {
            cmd.push("--model".to_string());
            cmd.push(opts.model.clone());
        }

        cmd.push(format!(
            "Read {} for your task assignment and begin immediately.",
            opts.instruction_path
        ));
        cmd
    }

    fn build_interactive_command(&self, opts: &SpawnOpts) -> String {
        let mut cmd = "codex --full-auto".to_string();
        if !should_omit_model(&opts.model) {
            cmd.push_str(&format!(" --model {}", opts.model));
        }
        cmd.push_str(&format!(
            " 'Read {} for your task assignment and begin immediately.'",
            opts.instruction_path
        ));
        cmd
    }

    fn deploy_config(
        &self,
        worktree: &Path,
        overlay_content: &str,
        _hooks: &HooksDef,
    ) -> Result<(), String> {
        // Codex reads AGENTS.md in the worktree root
        // No hooks deployment — Codex uses OS-level sandbox (Seatbelt/Landlock)
        if !overlay_content.is_empty() {
            let agents_path = worktree.join("AGENTS.md");
            fs::write(&agents_path, overlay_content)
                .map_err(|e| format!("Failed to write AGENTS.md: {e}"))?;
        }
        Ok(())
    }

    fn detect_ready(&self, _pane_content: &str) -> ReadyState {
        // Codex is always ready once spawned
        ReadyState {
            phase: ReadyPhase::Ready,
            detail: Some("Codex ready".to_string()),
        }
    }

    fn build_env(&self, model: &crate::types::ResolvedModel) -> HashMap<String, String> {
        model.env.clone().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ResolvedModel;
    use tempfile::TempDir;

    fn make_opts() -> SpawnOpts {
        SpawnOpts {
            model: "o4-mini".to_string(),
            cwd: "/tmp".to_string(),
            permission_mode: "bypass".to_string(),
            allowed_tools: vec!["Read".to_string(), "Write".to_string()],
            instruction_path: "AGENTS.md".to_string(),
        }
    }

    #[test]
    fn test_codex_id() {
        assert_eq!(CodexRuntime.id(), "codex");
    }

    #[test]
    fn test_codex_instruction_path() {
        assert_eq!(CodexRuntime.instruction_path(), "AGENTS.md");
    }

    #[test]
    fn test_codex_is_headless() {
        assert!(CodexRuntime.is_headless());
    }

    #[test]
    fn test_codex_headless_command_with_model() {
        let cmd = CodexRuntime.build_headless_command(&make_opts());
        assert_eq!(cmd[0], "codex");
        assert_eq!(cmd[1], "exec");
        assert_eq!(cmd[2], "--full-auto");
        assert_eq!(cmd[3], "--ephemeral");
        assert_eq!(cmd[4], "--model");
        assert_eq!(cmd[5], "o4-mini");
        assert!(cmd[6].contains("AGENTS.md"));
    }

    #[test]
    fn test_codex_headless_command_manifest_alias_omits_model() {
        let mut opts = make_opts();
        opts.model = "sonnet".to_string();
        let cmd = CodexRuntime.build_headless_command(&opts);
        assert!(!cmd.contains(&"--model".to_string()), "Should omit --model for manifest alias");
    }

    #[test]
    fn test_codex_headless_command_claude_model_omits_model() {
        let mut opts = make_opts();
        opts.model = "claude-sonnet-4-6".to_string();
        let cmd = CodexRuntime.build_headless_command(&opts);
        assert!(!cmd.contains(&"--model".to_string()), "Should omit --model for claude model");
    }

    #[test]
    fn test_codex_deploy_config_writes_agents_md() {
        let dir = TempDir::new().unwrap();
        let hooks = HooksDef {
            agent_name: "test".to_string(),
            capability: "builder".to_string(),
            worktree_path: dir.path().to_string_lossy().to_string(),
            quality_gates: None,
        };
        CodexRuntime
            .deploy_config(dir.path(), "# Codex overlay", &hooks)
            .unwrap();

        let agents_md = dir.path().join("AGENTS.md");
        assert!(agents_md.exists());
        assert_eq!(fs::read_to_string(agents_md).unwrap(), "# Codex overlay");

        // No .claude directory should be created
        assert!(!dir.path().join(".claude").exists());
    }

    #[test]
    fn test_codex_detect_ready_always() {
        let state = CodexRuntime.detect_ready("anything");
        assert_eq!(state.phase, ReadyPhase::Ready);
    }

    #[test]
    fn test_codex_build_env_passthrough() {
        let mut env = HashMap::new();
        env.insert("OPENAI_API_KEY".to_string(), "sk-test".to_string());
        let model = ResolvedModel {
            model: "gpt-4o".to_string(),
            env: Some(env),
            is_explicit_override: None,
        };
        let result = CodexRuntime.build_env(&model);
        assert_eq!(result.get("OPENAI_API_KEY").unwrap(), "sk-test");
    }
}
