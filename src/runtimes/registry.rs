#![allow(dead_code)]

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::{AgentRuntime, HooksDef, ReadyPhase, ReadyState, SpawnOpts};
use super::claude::ClaudeRuntime;

// ---------------------------------------------------------------------------
// Stub runtime macro
// ---------------------------------------------------------------------------

macro_rules! stub_runtime {
    ($name:ident, $id:literal, $path:literal, $headless:literal) => {
        pub struct $name;

        impl AgentRuntime for $name {
            fn id(&self) -> &str {
                $id
            }

            fn instruction_path(&self) -> &str {
                $path
            }

            fn is_headless(&self) -> bool {
                $headless
            }

            fn build_headless_command(&self, opts: &SpawnOpts) -> Vec<String> {
                vec![
                    $id.to_string(),
                    "-p".to_string(),
                    "--model".to_string(),
                    opts.model.clone(),
                ]
            }

            fn build_interactive_command(&self, opts: &SpawnOpts) -> String {
                format!("{} --model {}", $id, opts.model)
            }

            fn deploy_config(
                &self,
                worktree: &Path,
                overlay_content: &str,
                _hooks: &HooksDef,
            ) -> Result<(), String> {
                // Derive instruction dir from path (everything before last '/')
                let instr_path = $path;
                let overlay_file = worktree.join(instr_path);
                if let Some(parent) = overlay_file.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create dir: {e}"))?;
                }
                fs::write(&overlay_file, overlay_content)
                    .map_err(|e| format!("Failed to write overlay: {e}"))?;
                Ok(())
            }

            fn detect_ready(&self, _pane_content: &str) -> ReadyState {
                ReadyState {
                    phase: ReadyPhase::NotReady,
                    detail: Some("stub runtime".to_string()),
                }
            }

            fn build_env(&self, model: &crate::types::ResolvedModel) -> HashMap<String, String> {
                model.env.clone().unwrap_or_default()
            }
        }
    };
}

stub_runtime!(CodexRuntime,    "codex",    ".codex/AGENTS.md",                  true);
stub_runtime!(GeminiRuntime,   "gemini",   ".gemini/STYLE.md",                  true);
stub_runtime!(PiRuntime,       "pi",       "pi.md",                             false);
stub_runtime!(CopilotRuntime,  "copilot",  ".github/copilot-instructions.md",   true);
stub_runtime!(SaplingRuntime,  "sapling",  ".sapling/instructions.md",          false);
stub_runtime!(OpenCodeRuntime, "opencode", ".opencode/instructions.md",         true);

// ---------------------------------------------------------------------------
// Registry functions
// ---------------------------------------------------------------------------

pub fn get_runtime(name: &str) -> Result<Box<dyn AgentRuntime>, String> {
    match name {
        "claude"   => Ok(Box::new(ClaudeRuntime)),
        "codex"    => Ok(Box::new(CodexRuntime)),
        "gemini"   => Ok(Box::new(GeminiRuntime)),
        "pi"       => Ok(Box::new(PiRuntime)),
        "copilot"  => Ok(Box::new(CopilotRuntime)),
        "sapling"  => Ok(Box::new(SaplingRuntime)),
        "opencode" => Ok(Box::new(OpenCodeRuntime)),
        other      => Err(format!("Unknown runtime: {other}")),
    }
}

pub fn resolve_runtime(config: &crate::types::OverstoryConfig) -> Result<Box<dyn AgentRuntime>, String> {
    let name = config
        .runtime
        .as_ref()
        .map(|r| r.default.as_str())
        .unwrap_or("claude");
    get_runtime(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_runtime_claude() {
        let rt = get_runtime("claude").unwrap();
        assert_eq!(rt.id(), "claude");
    }

    #[test]
    fn test_get_runtime_all_stubs() {
        for name in &["claude", "codex", "gemini", "pi", "copilot", "sapling", "opencode"] {
            let rt = get_runtime(name).unwrap();
            assert_eq!(rt.id(), *name);
        }
    }

    #[test]
    fn test_get_runtime_unknown() {
        assert!(get_runtime("nonexistent").is_err());
    }

    #[test]
    fn test_resolve_runtime_default() {
        use crate::types::OverstoryConfig;
        // Default config has no runtime field → falls back to claude
        let config = OverstoryConfig::default();
        let rt = resolve_runtime(&config).unwrap();
        assert_eq!(rt.id(), "claude");
    }
}
