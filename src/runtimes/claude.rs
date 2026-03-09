#![allow(dead_code)]

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::{AgentRuntime, HooksDef, ReadyPhase, ReadyState, SpawnOpts};

pub struct ClaudeRuntime;

impl AgentRuntime for ClaudeRuntime {
    fn id(&self) -> &str {
        "claude"
    }

    fn instruction_path(&self) -> &str {
        ".claude/CLAUDE.md"
    }

    fn is_headless(&self) -> bool {
        true
    }

    fn build_headless_command(&self, opts: &SpawnOpts) -> Vec<String> {
        let tools_csv = opts.allowed_tools.join(",");
        vec![
            "claude".to_string(),
            "-p".to_string(),
            "--model".to_string(),
            opts.model.clone(),
            "--allowedTools".to_string(),
            tools_csv,
        ]
    }

    fn build_interactive_command(&self, opts: &SpawnOpts) -> String {
        let tools_csv = opts.allowed_tools.join(",");
        format!(
            "claude --model {} --allowedTools {} --permission-mode {}",
            opts.model, tools_csv, opts.permission_mode
        )
    }

    fn deploy_config(&self, worktree: &Path, overlay_content: &str, hooks: &HooksDef) -> Result<(), String> {
        let claude_dir = worktree.join(".claude");
        fs::create_dir_all(&claude_dir)
            .map_err(|e| format!("Failed to create .claude dir: {e}"))?;

        // Write overlay content to .claude/CLAUDE.md
        let overlay_path = claude_dir.join("CLAUDE.md");
        fs::write(&overlay_path, overlay_content)
            .map_err(|e| format!("Failed to write CLAUDE.md: {e}"))?;

        // Build settings.local.json with hooks
        let agent_name = &hooks.agent_name;
        let settings = serde_json::json!({
            "hooks": {
                "SessionStart": [
                    {
                        "hooks": [
                            {
                                "type": "command",
                                "command": format!("grove log session-start --agent {agent_name}")
                            }
                        ]
                    }
                ],
                "Stop": [
                    {
                        "hooks": [
                            {
                                "type": "command",
                                "command": format!("grove log session-end --agent {agent_name}")
                            }
                        ]
                    }
                ]
            }
        });

        let settings_path = claude_dir.join("settings.local.json");
        let settings_str = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("Failed to serialize settings: {e}"))?;
        fs::write(&settings_path, settings_str)
            .map_err(|e| format!("Failed to write settings.local.json: {e}"))?;

        Ok(())
    }

    fn detect_ready(&self, pane_content: &str) -> ReadyState {
        if pane_content.contains('❯') || pane_content.contains("bypass permissions") {
            ReadyState {
                phase: ReadyPhase::Ready,
                detail: Some("Claude Code TUI ready".to_string()),
            }
        } else {
            ReadyState {
                phase: ReadyPhase::NotReady,
                detail: None,
            }
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
            model: "claude-opus-4-6".to_string(),
            cwd: "/tmp".to_string(),
            permission_mode: "default".to_string(),
            allowed_tools: vec!["Read".to_string(), "Write".to_string()],
            instruction_path: ".claude/CLAUDE.md".to_string(),
        }
    }

    #[test]
    fn test_claude_id() {
        assert_eq!(ClaudeRuntime.id(), "claude");
    }

    #[test]
    fn test_claude_instruction_path() {
        assert_eq!(ClaudeRuntime.instruction_path(), ".claude/CLAUDE.md");
    }

    #[test]
    fn test_claude_is_headless() {
        assert!(ClaudeRuntime.is_headless());
    }

    #[test]
    fn test_claude_build_headless_command() {
        let cmd = ClaudeRuntime.build_headless_command(&make_opts());
        assert!(cmd.contains(&"claude".to_string()));
        assert!(cmd.contains(&"-p".to_string()));
        assert!(cmd.contains(&"--model".to_string()));
        assert!(cmd.contains(&"claude-opus-4-6".to_string()));
    }

    #[test]
    fn test_claude_detect_ready() {
        let ready = ClaudeRuntime.detect_ready("some output ❯");
        assert_eq!(ready.phase, ReadyPhase::Ready);

        let ready2 = ClaudeRuntime.detect_ready("bypass permissions check");
        assert_eq!(ready2.phase, ReadyPhase::Ready);

        let not_ready = ClaudeRuntime.detect_ready("loading...");
        assert_eq!(not_ready.phase, ReadyPhase::NotReady);
    }

    #[test]
    fn test_claude_deploy_config() {
        let dir = TempDir::new().unwrap();
        let hooks = HooksDef {
            agent_name: "test-agent".to_string(),
            capability: "builder".to_string(),
            worktree_path: dir.path().to_string_lossy().to_string(),
            quality_gates: None,
        };
        ClaudeRuntime.deploy_config(dir.path(), "# overlay content", &hooks).unwrap();

        let claude_md = dir.path().join(".claude/CLAUDE.md");
        assert!(claude_md.exists());
        let content = std::fs::read_to_string(&claude_md).unwrap();
        assert_eq!(content, "# overlay content");

        let settings = dir.path().join(".claude/settings.local.json");
        assert!(settings.exists());
    }

    #[test]
    fn test_claude_build_env_empty() {
        let model = ResolvedModel {
            model: "claude-opus-4-6".to_string(),
            env: None,
            is_explicit_override: None,
        };
        let env = ClaudeRuntime.build_env(&model);
        assert!(env.is_empty());
    }

    #[test]
    fn test_claude_build_env_with_values() {
        let mut map = HashMap::new();
        map.insert("ANTHROPIC_API_KEY".to_string(), "sk-test".to_string());
        let model = ResolvedModel {
            model: "claude-opus-4-6".to_string(),
            env: Some(map),
            is_explicit_override: None,
        };
        let env = ClaudeRuntime.build_env(&model);
        assert_eq!(env.get("ANTHROPIC_API_KEY").unwrap(), "sk-test");
    }
}
