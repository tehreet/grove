use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// === Model & Provider Types ===

/// Configuration for a model provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderConfig {
    pub r#type: String, // "native" | "gateway"
    #[serde(rename = "baseUrl", skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(rename = "authTokenEnv", skip_serializing_if = "Option::is_none")]
    pub auth_token_env: Option<String>,
}

/// Configuration for the Pi runtime's model alias expansion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PiRuntimeConfig {
    pub provider: String,
    pub model_map: HashMap<String, String>,
}

// === Task Tracker ===

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TaskTrackerBackend {
    #[default]
    Auto,
    Seeds,
    Beads,
}

// === Project Configuration ===

/// Conditions that trigger automatic coordinator shutdown.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatorExitTriggers {
    pub all_agents_done: bool,
    pub task_tracker_empty: bool,
    pub on_shutdown_signal: bool,
}

impl Default for CoordinatorExitTriggers {
    fn default() -> Self {
        Self {
            all_agents_done: false,
            task_tracker_empty: false,
            on_shutdown_signal: false,
        }
    }
}

/// A single quality gate command agents must pass before reporting completion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualityGate {
    pub name: String,
    pub command: String,
    pub description: String,
}

/// Browser verification settings for verifier agents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct VerificationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev_server_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewports: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConfig {
    pub name: String,
    pub root: String,
    pub canonical_branch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_gates: Option<Vec<QualityGate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<VerificationConfig>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            root: String::new(),
            canonical_branch: "main".to_string(),
            quality_gates: Some(default_quality_gates()),
            verification: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentsConfig {
    pub manifest_path: String,
    pub base_dir: String,
    pub max_concurrent: u32,
    pub stagger_delay_ms: u64,
    pub max_depth: u32,
    pub max_sessions_per_run: u32,
    pub max_agents_per_lead: u32,
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            manifest_path: ".overstory/agent-manifest.json".to_string(),
            base_dir: ".overstory/agent-defs".to_string(),
            max_concurrent: 25,
            stagger_delay_ms: 2_000,
            max_depth: 2,
            max_sessions_per_run: 0,
            max_agents_per_lead: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorktreesConfig {
    pub base_dir: String,
}

impl Default for WorktreesConfig {
    fn default() -> Self {
        Self {
            base_dir: ".overstory/worktrees".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskTrackerConfig {
    pub backend: TaskTrackerBackend,
    pub enabled: bool,
}

impl Default for TaskTrackerConfig {
    fn default() -> Self {
        Self {
            backend: TaskTrackerBackend::Auto,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MulchConfig {
    pub enabled: bool,
    pub domains: Vec<String>,
    pub prime_format: String,
}

impl Default for MulchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            domains: vec![],
            prime_format: "markdown".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MergeConfig {
    pub ai_resolve_enabled: bool,
    pub reimagine_enabled: bool,
}

impl Default for MergeConfig {
    fn default() -> Self {
        Self {
            ai_resolve_enabled: true,
            reimagine_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogConfig {
    pub tier0_enabled: bool,
    pub tier0_interval_ms: u64,
    pub tier1_enabled: bool,
    pub tier2_enabled: bool,
    pub stale_threshold_ms: u64,
    pub zombie_threshold_ms: u64,
    pub nudge_interval_ms: u64,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            tier0_enabled: true,
            tier0_interval_ms: 30_000,
            tier1_enabled: false,
            tier2_enabled: false,
            stale_threshold_ms: 300_000,
            zombie_threshold_ms: 600_000,
            nudge_interval_ms: 60_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatorConfig {
    pub exit_triggers: CoordinatorExitTriggers,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConfig {
    pub default: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<HashMap<String, String>>,
    #[serde(rename = "printCommand", skip_serializing_if = "Option::is_none")]
    pub print_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pi: Option<PiRuntimeConfig>,
    #[serde(rename = "shellInitDelayMs", skip_serializing_if = "Option::is_none")]
    pub shell_init_delay_ms: Option<u64>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            default: "claude".to_string(),
            capabilities: None,
            print_command: None,
            pi: Some(PiRuntimeConfig {
                provider: "anthropic".to_string(),
                model_map: [
                    ("opus".to_string(), "anthropic/claude-opus-4-6".to_string()),
                    ("sonnet".to_string(), "anthropic/claude-sonnet-4-6".to_string()),
                    ("haiku".to_string(), "anthropic/claude-haiku-4-5".to_string()),
                ]
                .into_iter()
                .collect(),
            }),
            shell_init_delay_ms: Some(0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoggingConfig {
    pub verbose: bool,
    pub redact_secrets: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            verbose: false,
            redact_secrets: true,
        }
    }
}

/// Full overstory configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OverstoryConfig {
    pub project: ProjectConfig,
    pub agents: AgentsConfig,
    pub worktrees: WorktreesConfig,
    #[serde(rename = "taskTracker")]
    pub task_tracker: TaskTrackerConfig,
    pub mulch: MulchConfig,
    pub merge: MergeConfig,
    pub providers: HashMap<String, ProviderConfig>,
    pub watchdog: WatchdogConfig,
    pub models: HashMap<String, String>,
    pub logging: LoggingConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinator: Option<CoordinatorConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeConfig>,
}

impl Default for OverstoryConfig {
    fn default() -> Self {
        let mut providers = HashMap::new();
        providers.insert(
            "anthropic".to_string(),
            ProviderConfig {
                r#type: "native".to_string(),
                base_url: None,
                auth_token_env: None,
            },
        );
        Self {
            project: ProjectConfig::default(),
            agents: AgentsConfig::default(),
            worktrees: WorktreesConfig::default(),
            task_tracker: TaskTrackerConfig::default(),
            mulch: MulchConfig::default(),
            merge: MergeConfig::default(),
            providers,
            watchdog: WatchdogConfig::default(),
            models: HashMap::new(),
            logging: LoggingConfig::default(),
            coordinator: None,
            runtime: Some(RuntimeConfig::default()),
        }
    }
}

pub fn default_quality_gates() -> Vec<QualityGate> {
    vec![
        QualityGate {
            name: "Tests".to_string(),
            command: "bun test".to_string(),
            description: "all tests must pass".to_string(),
        },
        QualityGate {
            name: "Lint".to_string(),
            command: "bun run lint".to_string(),
            description: "zero errors".to_string(),
        },
        QualityGate {
            name: "Typecheck".to_string(),
            command: "bun run typecheck".to_string(),
            description: "no TypeScript errors".to_string(),
        },
    ]
}

pub fn default_verification_config() -> VerificationConfig {
    VerificationConfig {
        dev_server_command: Some(String::new()),
        base_url: Some("http://localhost:3000".to_string()),
        port: Some(3000),
        routes: Some(vec!["/".to_string()]),
        viewports: Some(vec!["1280x720".to_string()]),
    }
}

// === Agent Manifest ===

/// All valid agent capability types.
pub const SUPPORTED_CAPABILITIES: &[&str] = &[
    "scout",
    "builder",
    "reviewer",
    "verifier",
    "lead",
    "merger",
    "coordinator",
    "supervisor",
    "monitor",
];

// === Agent Session ===

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Booting,
    Working,
    Completed,
    Stalled,
    Zombie,
}

// === Mail ===

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MailMessageType {
    Status,
    Question,
    Result,
    Error,
    WorkerDone,
    MergeReady,
    Merged,
    MergeFailed,
    Escalation,
    HealthCheck,
    Dispatch,
    Assign,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MailPriority {
    Low,
    Normal,
    High,
    Urgent,
}

// === Merge Queue ===

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ResolutionTier {
    CleanMerge,
    AutoResolve,
    AiResolve,
    Reimagine,
}

// === Events ===

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    ToolStart,
    ToolEnd,
    SessionStart,
    SessionEnd,
    MailSent,
    MailReceived,
    Spawn,
    Error,
    Custom,
    TurnStart,
    TurnEnd,
    Progress,
    Result,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EventLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_correct_values() {
        let config = OverstoryConfig::default();
        assert_eq!(config.project.canonical_branch, "main");
        assert_eq!(config.agents.max_concurrent, 25);
        assert_eq!(config.agents.max_depth, 2);
        assert_eq!(config.agents.stagger_delay_ms, 2_000);
        assert_eq!(config.agents.max_sessions_per_run, 0);
        assert_eq!(config.agents.max_agents_per_lead, 5);
        assert_eq!(config.watchdog.tier0_interval_ms, 30_000);
        assert_eq!(config.watchdog.stale_threshold_ms, 300_000);
        assert_eq!(config.watchdog.zombie_threshold_ms, 600_000);
        assert!(config.providers.contains_key("anthropic"));
    }

    #[test]
    fn quality_gates_have_defaults() {
        let gates = default_quality_gates();
        assert_eq!(gates.len(), 3);
        assert_eq!(gates[0].name, "Tests");
        assert_eq!(gates[1].name, "Lint");
        assert_eq!(gates[2].name, "Typecheck");
    }

    #[test]
    fn task_tracker_backend_serializes() {
        let backend = TaskTrackerBackend::Auto;
        let s = serde_json::to_string(&backend).unwrap();
        assert_eq!(s, "\"auto\"");
    }

    #[test]
    fn provider_config_roundtrip() {
        let p = ProviderConfig {
            r#type: "native".to_string(),
            base_url: None,
            auth_token_env: None,
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: ProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }
}
