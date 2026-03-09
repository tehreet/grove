use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

// === Type Aliases ===

pub type ModelRef = String;

// === Model & Provider Types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    #[serde(rename = "type")]
    pub provider_type: String, // "native" | "gateway"
    pub base_url: Option<String>,
    pub auth_token_env: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedModel {
    pub model: String,
    pub env: Option<HashMap<String, String>>,
    pub is_explicit_override: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiRuntimeConfig {
    pub provider: String,
    pub model_map: HashMap<String, String>,
}

// === Task Tracker ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskTrackerBackend {
    Auto,
    Seeds,
    Beads,
}

// === Project Configuration ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatorExitTriggers {
    pub all_agents_done: bool,
    pub task_tracker_empty: bool,
    pub on_shutdown_signal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityGate {
    pub name: String,
    pub command: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationConfig {
    pub dev_server_command: Option<String>,
    pub base_url: Option<String>,
    pub port: Option<u16>,
    pub routes: Option<Vec<String>>,
    pub viewports: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConfig {
    pub name: String,
    pub root: String,
    pub canonical_branch: String,
    pub quality_gates: Option<Vec<QualityGate>>,
    pub verification: Option<VerificationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreesConfig {
    pub base_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskTrackerConfig {
    pub backend: TaskTrackerBackend,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MulchConfig {
    pub enabled: bool,
    pub domains: Vec<String>,
    pub prime_format: String, // "markdown" | "xml" | "json"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeConfig {
    pub ai_resolve_enabled: bool,
    pub reimagine_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingConfig {
    pub verbose: bool,
    pub redact_secrets: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatorConfig {
    pub exit_triggers: CoordinatorExitTriggers,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConfig {
    pub default: String,
    pub capabilities: Option<HashMap<String, String>>,
    pub print_command: Option<String>,
    pub pi: Option<PiRuntimeConfig>,
    pub shell_init_delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverstoryConfig {
    pub project: ProjectConfig,
    pub agents: AgentsConfig,
    pub worktrees: WorktreesConfig,
    pub task_tracker: TaskTrackerConfig,
    pub mulch: MulchConfig,
    pub merge: MergeConfig,
    pub providers: HashMap<String, ProviderConfig>,
    pub watchdog: WatchdogConfig,
    pub models: HashMap<String, ModelRef>,
    pub logging: LoggingConfig,
    pub coordinator: Option<CoordinatorConfig>,
    pub runtime: Option<RuntimeConfig>,
}

// === Agent Manifest ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentManifest {
    pub version: String,
    pub agents: HashMap<String, AgentDefinition>,
    pub capability_index: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefinition {
    pub file: String,
    pub model: ModelRef,
    pub tools: Vec<String>,
    pub capabilities: Vec<String>,
    pub can_spawn: bool,
    pub constraints: Vec<String>,
}

// === Capability ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    Scout,
    Builder,
    Reviewer,
    Verifier,
    Lead,
    Merger,
    Coordinator,
    Supervisor,
    Monitor,
}

impl Capability {
    pub const ALL: &'static [Self] = &[
        Self::Scout,
        Self::Builder,
        Self::Reviewer,
        Self::Verifier,
        Self::Lead,
        Self::Merger,
        Self::Coordinator,
        Self::Supervisor,
        Self::Monitor,
    ];
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Scout => "scout",
            Self::Builder => "builder",
            Self::Reviewer => "reviewer",
            Self::Verifier => "verifier",
            Self::Lead => "lead",
            Self::Merger => "merger",
            Self::Coordinator => "coordinator",
            Self::Supervisor => "supervisor",
            Self::Monitor => "monitor",
        };
        write!(f, "{s}")
    }
}

// === Agent Session ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Booting,
    Working,
    Completed,
    Stalled,
    Zombie,
}

impl fmt::Display for AgentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Booting => "booting",
            Self::Working => "working",
            Self::Completed => "completed",
            Self::Stalled => "stalled",
            Self::Zombie => "zombie",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub id: String,
    pub agent_name: String,
    pub capability: String,
    pub worktree_path: String,
    pub branch_name: String,
    pub task_id: String,
    pub tmux_session: String,
    pub state: AgentState,
    pub pid: Option<i64>,
    pub parent_agent: Option<String>,
    pub depth: u32,
    pub run_id: Option<String>,
    pub started_at: String,
    pub last_activity: String,
    pub escalation_level: u32,
    pub stalled_since: Option<String>,
    pub transcript_path: Option<String>,
}

// === Agent Identity ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentTask {
    pub task_id: String,
    pub summary: String,
    pub completed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentIdentity {
    pub name: String,
    pub capability: String,
    pub created: String,
    pub sessions_completed: u32,
    pub expertise_domains: Vec<String>,
    pub recent_tasks: Vec<RecentTask>,
}

// === Mail ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

impl MailMessageType {
    pub const ALL: &'static [Self] = &[
        Self::Status,
        Self::Question,
        Self::Result,
        Self::Error,
        Self::WorkerDone,
        Self::MergeReady,
        Self::Merged,
        Self::MergeFailed,
        Self::Escalation,
        Self::HealthCheck,
        Self::Dispatch,
        Self::Assign,
    ];
}

impl fmt::Display for MailMessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Status => "status",
            Self::Question => "question",
            Self::Result => "result",
            Self::Error => "error",
            Self::WorkerDone => "worker_done",
            Self::MergeReady => "merge_ready",
            Self::Merged => "merged",
            Self::MergeFailed => "merge_failed",
            Self::Escalation => "escalation",
            Self::HealthCheck => "health_check",
            Self::Dispatch => "dispatch",
            Self::Assign => "assign",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MailPriority {
    Low,
    Normal,
    High,
    Urgent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub priority: MailPriority,
    #[serde(rename = "type")]
    pub message_type: MailMessageType,
    pub thread_id: Option<String>,
    pub payload: Option<String>,
    pub read: bool,
    pub created_at: String,
}

// === Mail Protocol Payloads ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerDonePayload {
    pub task_id: String,
    pub branch: String,
    pub exit_code: i32,
    pub files_modified: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeReadyPayload {
    pub branch: String,
    pub task_id: String,
    pub agent_name: String,
    pub files_modified: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergedPayload {
    pub branch: String,
    pub task_id: String,
    pub tier: ResolutionTier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeFailedPayload {
    pub branch: String,
    pub task_id: String,
    pub conflict_files: Vec<String>,
    pub error_message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EscalationSeverity {
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EscalationPayload {
    pub severity: EscalationSeverity,
    pub task_id: Option<String>,
    pub context: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckType {
    Liveness,
    Readiness,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheckPayload {
    pub agent_name: String,
    pub check_type: CheckType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DispatchPayload {
    pub task_id: String,
    pub spec_path: String,
    pub capability: Capability,
    pub file_scope: Vec<String>,
    pub skip_scouts: Option<bool>,
    pub skip_review: Option<bool>,
    pub max_agents: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignPayload {
    pub task_id: String,
    pub spec_path: String,
    pub worker_name: String,
    pub branch: String,
}

// === Mulch Records ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MulchRecordClassification {
    Foundational,
    Tactical,
    Observational,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MulchRecordOutcome {
    pub status: String, // "success" | "failure" | "partial"
    pub agent: Option<String>,
    pub notes: Option<String>,
    pub recorded_at: Option<String>,
}

/// Discriminated union matching the TS MulchRecord tagged union.
/// Fields use snake_case to match the actual mulch JSON format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MulchRecord {
    #[serde(rename = "convention")]
    Convention {
        content: String,
        classification: MulchRecordClassification,
        recorded_at: String,
        id: Option<String>,
        outcomes: Option<Vec<MulchRecordOutcome>>,
        supersedes: Option<Vec<String>>,
    },
    #[serde(rename = "pattern")]
    Pattern {
        name: String,
        description: String,
        classification: MulchRecordClassification,
        recorded_at: String,
        id: Option<String>,
        outcomes: Option<Vec<MulchRecordOutcome>>,
        supersedes: Option<Vec<String>>,
    },
    #[serde(rename = "failure")]
    Failure {
        description: String,
        resolution: String,
        classification: MulchRecordClassification,
        recorded_at: String,
        id: Option<String>,
        outcomes: Option<Vec<MulchRecordOutcome>>,
        supersedes: Option<Vec<String>>,
    },
    #[serde(rename = "decision")]
    Decision {
        title: String,
        rationale: String,
        classification: MulchRecordClassification,
        recorded_at: String,
        id: Option<String>,
        outcomes: Option<Vec<MulchRecordOutcome>>,
        supersedes: Option<Vec<String>>,
    },
    #[serde(rename = "reference")]
    Reference {
        name: String,
        description: String,
        classification: MulchRecordClassification,
        recorded_at: String,
        id: Option<String>,
        outcomes: Option<Vec<MulchRecordOutcome>>,
        supersedes: Option<Vec<String>>,
    },
    #[serde(rename = "guide")]
    Guide {
        name: String,
        description: String,
        classification: MulchRecordClassification,
        recorded_at: String,
        id: Option<String>,
        outcomes: Option<Vec<MulchRecordOutcome>>,
        supersedes: Option<Vec<String>>,
    },
}

// === Overlay ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayConfig {
    pub agent_name: String,
    pub task_id: String,
    pub spec_path: Option<String>,
    pub branch_name: String,
    pub worktree_path: String,
    pub file_scope: Vec<String>,
    pub mulch_domains: Vec<String>,
    pub parent_agent: Option<String>,
    pub depth: u32,
    pub can_spawn: bool,
    pub capability: String,
    pub base_definition: String,
    pub mulch_expertise: Option<String>,
    pub mulch_records: Option<Vec<MulchRecord>>,
    pub no_directives: Option<bool>,
    pub skip_scout: Option<bool>,
    pub skip_review: Option<bool>,
    pub max_agents_override: Option<u32>,
    pub tracker_cli: Option<String>,
    pub tracker_name: Option<String>,
    pub quality_gates: Option<Vec<QualityGate>>,
    pub instruction_path: Option<String>,
    pub verification: Option<VerificationConfig>,
}

// === Merge Queue ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolutionTier {
    CleanMerge,
    AutoResolve,
    AiResolve,
    Reimagine,
}

impl fmt::Display for ResolutionTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::CleanMerge => "clean-merge",
            Self::AutoResolve => "auto-resolve",
            Self::AiResolve => "ai-resolve",
            Self::Reimagine => "reimagine",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeEntryStatus {
    Pending,
    Merging,
    Merged,
    Conflict,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeEntry {
    pub branch_name: String,
    pub task_id: String,
    pub agent_name: String,
    pub files_modified: Vec<String>,
    pub enqueued_at: String,
    pub status: MergeEntryStatus,
    pub resolved_tier: Option<ResolutionTier>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeResult {
    pub entry: MergeEntry,
    pub success: bool,
    pub tier: ResolutionTier,
    pub conflict_files: Vec<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedConflictPattern {
    pub tier: ResolutionTier,
    pub success: bool,
    pub files: Vec<String>,
    pub agent: String,
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictHistory {
    pub skip_tiers: Vec<ResolutionTier>,
    pub past_resolutions: Vec<String>,
    pub predicted_conflict_files: Vec<String>,
}

// === Watchdog ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WatchdogAction {
    None,
    Escalate,
    Terminate,
    Investigate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    pub agent_name: String,
    pub timestamp: String,
    pub process_alive: bool,
    pub tmux_alive: bool,
    pub pid_alive: Option<bool>,
    pub last_activity: String,
    pub state: AgentState,
    pub action: WatchdogAction,
    pub reconciliation_note: Option<String>,
}

// === Logging ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEvent {
    pub timestamp: String,
    pub level: LogLevel,
    pub event: String,
    pub agent_name: Option<String>,
    pub data: HashMap<String, serde_json::Value>,
}

// === Metrics ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetrics {
    pub agent_name: String,
    pub task_id: String,
    pub capability: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub duration_ms: i64,
    pub exit_code: Option<i64>,
    pub merge_result: Option<ResolutionTier>,
    pub parent_agent: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub estimated_cost_usd: Option<f64>,
    pub model_used: Option<String>,
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenSnapshot {
    pub agent_name: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub estimated_cost_usd: Option<f64>,
    pub model_used: Option<String>,
    pub created_at: String,
    pub run_id: Option<String>,
}

// === Task Groups ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskGroupStatus {
    Active,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskGroup {
    pub id: String,
    pub name: String,
    pub member_issue_ids: Vec<String>,
    pub status: TaskGroupStatus,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskGroupProgress {
    pub group: TaskGroup,
    pub total: u32,
    pub completed: u32,
    pub in_progress: u32,
    pub blocked: u32,
    pub open: u32,
}

// === Events ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl EventLevel {
    pub const ALL: &'static [Self] = &[Self::Debug, Self::Info, Self::Warn, Self::Error];
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredEvent {
    pub id: i64,
    pub run_id: Option<String>,
    pub agent_name: String,
    pub session_id: Option<String>,
    pub event_type: EventType,
    pub tool_name: Option<String>,
    pub tool_args: Option<String>,
    pub tool_duration_ms: Option<i64>,
    pub level: EventLevel,
    pub data: Option<String>,
    pub created_at: String,
}

/// Input for inserting a new event (id and created_at are auto-generated).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertEvent {
    pub run_id: Option<String>,
    pub agent_name: String,
    pub session_id: Option<String>,
    pub event_type: EventType,
    pub tool_name: Option<String>,
    pub tool_args: Option<String>,
    pub tool_duration_ms: Option<i64>,
    pub level: EventLevel,
    pub data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventQueryOptions {
    pub limit: Option<u32>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub level: Option<EventLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStats {
    pub tool_name: String,
    pub count: i64,
    pub avg_duration_ms: f64,
    pub max_duration_ms: f64,
}

// === Run ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Active,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub agent_count: u32,
    pub coordinator_session_id: Option<String>,
    pub status: RunStatus,
}

/// Input for creating a new run (completedAt omitted; agentCount optional).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertRun {
    pub id: String,
    pub started_at: String,
    pub coordinator_session_id: Option<String>,
    pub status: RunStatus,
    pub agent_count: Option<u32>,
}

// === Mulch CLI Results ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainStatus {
    pub name: String,
    pub record_count: u32,
    pub last_updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MulchStatus {
    pub domains: Vec<DomainStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MulchDiffResult {
    pub success: bool,
    pub command: String,
    pub since: String,
    pub domains: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MulchLearnResult {
    pub success: bool,
    pub command: String,
    pub changed_files: Vec<String>,
    pub suggested_domains: Vec<String>,
    pub unmatched_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PruneResultItem {
    pub domain: String,
    pub pruned: u32,
    pub records: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MulchPruneResult {
    pub success: bool,
    pub command: String,
    pub dry_run: bool,
    pub total_pruned: u32,
    pub results: Vec<PruneResultItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DoctorCheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorCheck {
    pub name: String,
    pub status: DoctorCheckStatus,
    pub message: String,
    pub fixable: bool,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorSummary {
    pub pass: u32,
    pub warn: u32,
    pub fail: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MulchDoctorResult {
    pub success: bool,
    pub command: String,
    pub checks: Vec<DoctorCheck>,
    pub summary: DoctorSummary,
}

/// An entry from mulch ready — uses snake_case to match mulch JSON format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadyEntry {
    pub domain: String,
    pub id: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub recorded_at: String,
    pub summary: String,
    pub record: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MulchReadyResult {
    pub success: bool,
    pub command: String,
    pub count: u32,
    pub entries: Vec<ReadyEntry>,
}

/// A candidate group for compaction — uses snake_case to match mulch JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactRecord {
    pub id: String,
    pub summary: String,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactCandidate {
    pub domain: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub records: Vec<CompactRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactedItem {
    pub domain: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub before: u32,
    pub after: u32,
    pub record_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MulchCompactResult {
    pub success: bool,
    pub command: String,
    pub action: String,
    pub candidates: Option<Vec<CompactCandidate>>,
    pub compacted: Option<Vec<CompactedItem>>,
    pub message: Option<String>,
}

// === Session Lifecycle ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCheckpoint {
    pub agent_name: String,
    pub task_id: String,
    pub session_id: String,
    pub timestamp: String,
    pub progress_summary: String,
    pub files_modified: Vec<String>,
    pub current_branch: String,
    pub pending_work: String,
    pub mulch_domains: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HandoffReason {
    Compaction,
    Crash,
    Manual,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHandoff {
    pub from_session_id: String,
    pub to_session_id: Option<String>,
    pub checkpoint: SessionCheckpoint,
    pub reason: HandoffReason,
    pub handoff_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSandbox {
    pub worktree_path: String,
    pub branch_name: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLayerSession {
    pub id: String,
    pub pid: Option<i64>,
    pub tmux_session: String,
    pub started_at: String,
    pub checkpoint: Option<SessionCheckpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLayers {
    pub identity: AgentIdentity,
    pub sandbox: AgentSandbox,
    pub session: Option<AgentLayerSession>,
}

// === Session Insight Analysis ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInsight {
    #[serde(rename = "type")]
    pub insight_type: String, // "pattern" | "convention" | "failure"
    pub domain: String,
    pub description: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUsage {
    pub name: String,
    pub count: u32,
    pub avg_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolProfile {
    pub top_tools: Vec<ToolUsage>,
    pub total_tool_calls: u32,
    pub error_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotFile {
    pub path: String,
    pub edit_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProfile {
    pub hot_files: Vec<HotFile>,
    pub total_edits: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightAnalysis {
    pub insights: Vec<SessionInsight>,
    pub tool_profile: ToolProfile,
    pub file_profile: FileProfile,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip<T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug>(
        value: T,
    ) {
        let json = serde_json::to_string(&value).expect("serialize");
        let back: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(value, back);
    }

    #[test]
    fn agent_state_roundtrip() {
        roundtrip(AgentState::Booting);
        roundtrip(AgentState::Working);
        roundtrip(AgentState::Completed);
        roundtrip(AgentState::Stalled);
        roundtrip(AgentState::Zombie);
    }

    #[test]
    fn agent_state_display() {
        assert_eq!(AgentState::Working.to_string(), "working");
        assert_eq!(AgentState::Zombie.to_string(), "zombie");
    }

    #[test]
    fn agent_state_serializes_lowercase() {
        let json = serde_json::to_string(&AgentState::Booting).unwrap();
        assert_eq!(json, r#""booting""#);
    }

    #[test]
    fn capability_roundtrip() {
        for cap in Capability::ALL {
            roundtrip(*cap);
        }
    }

    #[test]
    fn capability_display() {
        assert_eq!(Capability::Builder.to_string(), "builder");
        assert_eq!(Capability::Coordinator.to_string(), "coordinator");
    }

    #[test]
    fn capability_all_count() {
        assert_eq!(Capability::ALL.len(), 9);
    }

    #[test]
    fn mail_message_type_roundtrip() {
        for mt in MailMessageType::ALL {
            roundtrip(*mt);
        }
    }

    #[test]
    fn mail_message_type_snake_case() {
        let json = serde_json::to_string(&MailMessageType::WorkerDone).unwrap();
        assert_eq!(json, r#""worker_done""#);
        let json = serde_json::to_string(&MailMessageType::MergeFailed).unwrap();
        assert_eq!(json, r#""merge_failed""#);
        let json = serde_json::to_string(&MailMessageType::HealthCheck).unwrap();
        assert_eq!(json, r#""health_check""#);
    }

    #[test]
    fn mail_message_type_display() {
        assert_eq!(MailMessageType::WorkerDone.to_string(), "worker_done");
        assert_eq!(MailMessageType::Status.to_string(), "status");
    }

    #[test]
    fn resolution_tier_roundtrip() {
        roundtrip(ResolutionTier::CleanMerge);
        roundtrip(ResolutionTier::AutoResolve);
        roundtrip(ResolutionTier::AiResolve);
        roundtrip(ResolutionTier::Reimagine);
    }

    #[test]
    fn resolution_tier_kebab_case() {
        let json = serde_json::to_string(&ResolutionTier::CleanMerge).unwrap();
        assert_eq!(json, r#""clean-merge""#);
        let json = serde_json::to_string(&ResolutionTier::AiResolve).unwrap();
        assert_eq!(json, r#""ai-resolve""#);
    }

    #[test]
    fn resolution_tier_display() {
        assert_eq!(ResolutionTier::CleanMerge.to_string(), "clean-merge");
        assert_eq!(ResolutionTier::AiResolve.to_string(), "ai-resolve");
    }

    #[test]
    fn event_type_roundtrip() {
        let cases = [
            (EventType::ToolStart, "tool_start"),
            (EventType::ToolEnd, "tool_end"),
            (EventType::SessionStart, "session_start"),
            (EventType::TurnStart, "turn_start"),
            (EventType::MailSent, "mail_sent"),
        ];
        for (et, expected) in cases {
            let json = serde_json::to_string(&et).unwrap();
            assert_eq!(json, format!(r#""{expected}""#));
            let back: EventType = serde_json::from_str(&json).unwrap();
            assert_eq!(et, back);
        }
    }

    #[test]
    fn event_level_all() {
        assert_eq!(EventLevel::ALL.len(), 4);
        for level in EventLevel::ALL {
            let json = serde_json::to_string(level).unwrap();
            let back: EventLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(*level, back);
        }
    }

    #[test]
    fn mail_message_roundtrip() {
        let msg = MailMessage {
            id: "msg-abc123".into(),
            from: "builder-1".into(),
            to: "types-lead".into(),
            subject: "done".into(),
            body: "Completed task".into(),
            priority: MailPriority::Normal,
            message_type: MailMessageType::WorkerDone,
            thread_id: None,
            payload: Some(r#"{"taskId":"t-1"}"#.into()),
            read: false,
            created_at: "2026-01-01T00:00:00.000Z".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        // Verify the "type" field is present (not "messageType")
        assert!(json.contains(r#""type":"worker_done""#));
        // Verify camelCase fields
        assert!(json.contains(r#""createdAt""#));
        assert!(json.contains(r#""threadId""#));
        let back: MailMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, msg.id);
        assert_eq!(back.message_type, msg.message_type);
    }

    #[test]
    fn agent_session_roundtrip() {
        let session = AgentSession {
            id: "sess-1".into(),
            agent_name: "builder-1".into(),
            capability: "builder".into(),
            worktree_path: "/tmp/wt".into(),
            branch_name: "feat/x".into(),
            task_id: "t-1".into(),
            tmux_session: "tmux-1".into(),
            state: AgentState::Working,
            pid: Some(1234),
            parent_agent: Some("lead-1".into()),
            depth: 1,
            run_id: Some("run-1".into()),
            started_at: "2026-01-01T00:00:00.000Z".into(),
            last_activity: "2026-01-01T00:01:00.000Z".into(),
            escalation_level: 0,
            stalled_since: None,
            transcript_path: None,
        };
        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains(r#""agentName""#));
        assert!(json.contains(r#""worktreePath""#));
        let back: AgentSession = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, session.id);
        assert_eq!(back.state, session.state);
        assert_eq!(back.pid, session.pid);
    }

    #[test]
    fn provider_config_camel_case() {
        let cfg = ProviderConfig {
            provider_type: "native".into(),
            base_url: Some("https://example.com".into()),
            auth_token_env: Some("API_KEY".into()),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains(r#""type":"native""#));
        assert!(json.contains(r#""baseUrl""#));
        assert!(json.contains(r#""authTokenEnv""#));
    }

    #[test]
    fn mulch_record_tagged_union() {
        let record = MulchRecord::Convention {
            content: "Always use WAL mode".into(),
            classification: MulchRecordClassification::Foundational,
            recorded_at: "2026-01-01T00:00:00.000Z".into(),
            id: Some("rec-1".into()),
            outcomes: None,
            supersedes: None,
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains(r#""type":"convention""#));
        assert!(json.contains(r#""recorded_at""#));
        let back: MulchRecord = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, MulchRecord::Convention { .. }));

        let pattern = MulchRecord::Pattern {
            name: "Error wrapping".into(),
            description: "Use thiserror".into(),
            classification: MulchRecordClassification::Tactical,
            recorded_at: "2026-01-01T00:00:00.000Z".into(),
            id: None,
            outcomes: None,
            supersedes: None,
        };
        let json2 = serde_json::to_string(&pattern).unwrap();
        assert!(json2.contains(r#""type":"pattern""#));
    }

    #[test]
    fn session_checkpoint_roundtrip() {
        let cp = SessionCheckpoint {
            agent_name: "builder-1".into(),
            task_id: "t-1".into(),
            session_id: "sess-1".into(),
            timestamp: "2026-01-01T00:00:00.000Z".into(),
            progress_summary: "50% done".into(),
            files_modified: vec!["src/main.rs".into()],
            current_branch: "feat/x".into(),
            pending_work: "Write tests".into(),
            mulch_domains: vec!["rust".into()],
        };
        let json = serde_json::to_string(&cp).unwrap();
        assert!(json.contains(r#""agentName""#));
        assert!(json.contains(r#""progressSummary""#));
        let back: SessionCheckpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(back.agent_name, cp.agent_name);
        assert_eq!(back.files_modified, cp.files_modified);
    }

    #[test]
    fn handoff_reason_variants() {
        let reasons = [
            (HandoffReason::Compaction, "compaction"),
            (HandoffReason::Crash, "crash"),
            (HandoffReason::Manual, "manual"),
            (HandoffReason::Timeout, "timeout"),
        ];
        for (reason, expected) in reasons {
            let json = serde_json::to_string(&reason).unwrap();
            assert_eq!(json, format!(r#""{expected}""#));
            let back: HandoffReason = serde_json::from_str(&json).unwrap();
            assert_eq!(reason, back);
        }
    }
}
