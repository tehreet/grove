//! Runtime registry — maps runtime names to adapter instances.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::{AgentRuntime, HooksDef, ReadyPhase, ReadyState, SpawnOpts};
use super::claude::ClaudeRuntime;
use super::codex::CodexRuntime;
use super::copilot::CopilotRuntime;
use super::gemini::GeminiRuntime;

/// Resolve a runtime adapter by name.
///
/// Supported runtimes: claude, codex, gemini, copilot
///
/// Additional runtimes (pi, sapling, opencode) use the stub fallback
/// which builds a basic command from the runtime name.
pub fn get_runtime(name: &str) -> Result<Box<dyn AgentRuntime>, String> {
    match name {
        "claude"   => Ok(Box::new(ClaudeRuntime)),
        "codex"    => Ok(Box::new(CodexRuntime)),
        "gemini"   => Ok(Box::new(GeminiRuntime)),
        "copilot"  => Ok(Box::new(CopilotRuntime)),
        // Stub runtimes for less common adapters — basic command generation
        "pi"       => Ok(Box::new(StubRuntime::new("pi", ".claude/CLAUDE.md", "pi --print"))),
        "sapling"  => Ok(Box::new(StubRuntime::new("sapling", "SAPLING.md", "sp print"))),
        "opencode" => Ok(Box::new(StubRuntime::new("opencode", ".opencode/instructions.md", "opencode --prompt"))),
        other      => Err(format!(
            "Unknown runtime: \"{other}\". Available: claude, codex, gemini, copilot, pi, sapling, opencode"
        )),
    }
}

/// Resolve runtime from config, with optional per-capability routing.
///
/// Lookup order:
/// 1. `config.runtime.capabilities[capability]` (if capability provided)
/// 2. `config.runtime.default`
/// 3. `"claude"` (hardcoded fallback)
pub fn resolve_runtime_for(
    config: &crate::types::OverstoryConfig,
    capability: Option<&str>,
) -> Result<Box<dyn AgentRuntime>, String> {
    // Check per-capability routing first
    if let Some(cap) = capability {
        if let Some(ref rt_config) = config.runtime {
            if let Some(ref caps) = rt_config.capabilities {
                if let Some(rt_name) = caps.get(cap) {
                    return get_runtime(rt_name);
                }
            }
        }
    }

    // Fall back to default
    let name = config
        .runtime
        .as_ref()
        .map(|r| r.default.as_str())
        .unwrap_or("claude");
    get_runtime(name)
}

/// List all available runtime names.
pub fn available_runtimes() -> Vec<&'static str> {
    vec!["claude", "codex", "gemini", "copilot", "pi", "sapling", "opencode"]
}

// ---------------------------------------------------------------------------
// Stub runtime for less common adapters
// ---------------------------------------------------------------------------

/// Generic stub runtime for adapters that aren't fully implemented yet.
/// Generates basic commands from the runtime name and instruction path.
struct StubRuntime {
    name: String,
    instr_path: String,
    headless_prefix: String,
}

impl StubRuntime {
    fn new(name: &str, instr_path: &str, headless_prefix: &str) -> Self {
        Self {
            name: name.to_string(),
            instr_path: instr_path.to_string(),
            headless_prefix: headless_prefix.to_string(),
        }
    }
}

impl AgentRuntime for StubRuntime {
    fn id(&self) -> &str {
        &self.name
    }

    fn instruction_path(&self) -> &str {
        &self.instr_path
    }

    fn is_headless(&self) -> bool {
        true
    }

    fn build_headless_command(&self, opts: &SpawnOpts) -> Vec<String> {
        let mut parts: Vec<String> = self.headless_prefix
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        parts.push(format!(
            "Read {} for your task assignment and begin immediately.",
            opts.instruction_path
        ));
        parts
    }

    fn build_interactive_command(&self, opts: &SpawnOpts) -> String {
        format!("{} --model {}", self.name, opts.model)
    }

    fn deploy_config(
        &self,
        worktree: &Path,
        overlay_content: &str,
        _hooks: &HooksDef,
    ) -> Result<(), String> {
        if !overlay_content.is_empty() {
            let overlay_file = worktree.join(&self.instr_path);
            if let Some(parent) = overlay_file.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create dir: {e}"))?;
            }
            fs::write(&overlay_file, overlay_content)
                .map_err(|e| format!("Failed to write overlay: {e}"))?;
        }
        Ok(())
    }

    fn detect_ready(&self, _pane_content: &str) -> ReadyState {
        ReadyState {
            phase: ReadyPhase::Ready,
            detail: Some(format!("{} ready", self.name)),
        }
    }

    fn build_env(&self, model: &crate::types::ResolvedModel) -> HashMap<String, String> {
        model.env.clone().unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_runtime_all() {
        for name in available_runtimes() {
            let rt = get_runtime(name).unwrap();
            assert_eq!(rt.id(), name);
        }
    }

    #[test]
    fn test_get_runtime_unknown() {
        assert!(get_runtime("nonexistent").is_err());
    }

    #[test]
    fn test_codex_instruction_path() {
        let rt = get_runtime("codex").unwrap();
        assert_eq!(rt.instruction_path(), "AGENTS.md");
    }

    #[test]
    fn test_gemini_instruction_path() {
        let rt = get_runtime("gemini").unwrap();
        assert_eq!(rt.instruction_path(), "GEMINI.md");
    }

    #[test]
    fn test_copilot_instruction_path() {
        let rt = get_runtime("copilot").unwrap();
        assert_eq!(rt.instruction_path(), ".github/copilot-instructions.md");
    }

    #[test]
    fn test_per_capability_routing() {
        let mut config = crate::types::OverstoryConfig::default();
        let mut rt_config = crate::types::RuntimeConfig {
            default: "claude".to_string(),
            capabilities: None,
            print_command: None,
            pi: None,
            shell_init_delay_ms: None,
        };
        let mut caps = HashMap::new();
        caps.insert("builder".to_string(), "codex".to_string());
        rt_config.capabilities = Some(caps);
        config.runtime = Some(rt_config);

        // Builder should get codex
        let rt = resolve_runtime_for(&config, Some("builder")).unwrap();
        assert_eq!(rt.id(), "codex");

        // Lead should fall back to claude (default)
        let rt = resolve_runtime_for(&config, Some("lead")).unwrap();
        assert_eq!(rt.id(), "claude");

        // No capability should fall back to claude
        let rt = resolve_runtime_for(&config, None).unwrap();
        assert_eq!(rt.id(), "claude");
    }
}
