#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;

pub mod claude;
pub mod codex;
pub mod copilot;
pub mod gemini;
pub mod registry;

/// Ready-state detection for interactive runtimes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadyPhase {
    NotReady,
    Ready,
    Processing,
}

#[derive(Debug, Clone)]
pub struct ReadyState {
    pub phase: ReadyPhase,
    pub detail: Option<String>,
}

/// Options passed to build_headless_command and build_interactive_command.
#[derive(Debug, Clone)]
pub struct SpawnOpts {
    pub model: String,
    pub cwd: String,
    pub permission_mode: String,
    pub allowed_tools: Vec<String>,
    pub instruction_path: String,
}

/// Hook definitions for deploy_config.
#[derive(Debug, Clone, Default)]
pub struct HooksDef {
    pub agent_name: String,
    pub capability: String,
    pub worktree_path: String,
    pub quality_gates: Option<Vec<crate::types::QualityGate>>,
}

/// Core trait that each runtime adapter implements.
pub trait AgentRuntime: Send + Sync {
    /// Runtime identifier (e.g., "claude", "codex", "gemini")
    fn id(&self) -> &str;

    /// Path to the instruction/overlay file relative to worktree root
    fn instruction_path(&self) -> &str;

    /// Whether this runtime is headless (direct process, no tmux)
    fn is_headless(&self) -> bool;

    /// Build argv for headless (stdin/stdout pipe) spawn
    fn build_headless_command(&self, opts: &SpawnOpts) -> Vec<String>;

    /// Build shell command string for interactive spawn
    fn build_interactive_command(&self, opts: &SpawnOpts) -> String;

    /// Deploy overlay + hooks config to worktree
    fn deploy_config(&self, worktree: &Path, overlay_content: &str, hooks: &HooksDef) -> Result<(), String>;

    /// Detect readiness from output content
    fn detect_ready(&self, pane_content: &str) -> ReadyState;

    /// Build environment variables for model/provider routing
    fn build_env(&self, model: &crate::types::ResolvedModel) -> HashMap<String, String>;
}
