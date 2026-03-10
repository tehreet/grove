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

    fn build_print_command(&self, prompt: &str, model: Option<&str>) -> Vec<String> {
        let mut cmd = vec!["copilot".to_string(), "-p".to_string()];
        if let Some(model) = model {
            cmd.push("--model".to_string());
            cmd.push(model.to_string());
        }
        cmd.push(prompt.to_string());
        cmd.push("--allow-all-tools".to_string());
        cmd
    }

    fn parse_transcript(&self, path: &Path) -> Option<crate::types::TranscriptSummary> {
        let content = fs::read_to_string(path).ok()?;
        let mut summary = crate::types::TranscriptSummary::default();
        let mut found = false;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };

            if let Some(usage) = val.get("usage") {
                found = true;
                summary.input_tokens += usage
                    .get("prompt_tokens")
                    .or_else(|| usage.get("input_tokens"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                summary.output_tokens += usage
                    .get("completion_tokens")
                    .or_else(|| usage.get("output_tokens"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
            }

            if let Some(model) = val.get("model").and_then(serde_json::Value::as_str) {
                if !model.is_empty() {
                    summary.model = Some(model.to_string());
                }
            }
        }

        found.then_some(summary)
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
        assert_eq!(
            CopilotRuntime.instruction_path(),
            ".github/copilot-instructions.md"
        );
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

    #[test]
    fn test_build_print_command_no_model() {
        let cmd = CopilotRuntime.build_print_command("hello world", None);
        assert_eq!(
            cmd,
            vec!["copilot", "-p", "hello world", "--allow-all-tools"]
        );
    }

    #[test]
    fn test_build_print_command_with_model() {
        let cmd = CopilotRuntime.build_print_command("hello", Some("gpt-4.1"));
        assert_eq!(
            cmd,
            vec![
                "copilot",
                "-p",
                "--model",
                "gpt-4.1",
                "hello",
                "--allow-all-tools"
            ]
        );
    }

    #[test]
    fn test_parse_transcript_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("copilot.jsonl");
        std::fs::write(&path, "").unwrap();
        assert!(CopilotRuntime.parse_transcript(&path).is_none());
    }

    #[test]
    fn test_parse_transcript_with_usage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("copilot.jsonl");
        std::fs::write(
            &path,
            r#"{"usage":{"prompt_tokens":18,"completion_tokens":7},"model":"gpt-4.1"}"#,
        )
        .unwrap();
        let summary = CopilotRuntime.parse_transcript(&path).unwrap();
        assert_eq!(summary.input_tokens, 18);
        assert_eq!(summary.output_tokens, 7);
        assert_eq!(summary.model.as_deref(), Some("gpt-4.1"));
    }
}
