//! Grove CLI — multi-agent orchestration for AI coding agents.
//!
//! Port of `reference/index.ts`. All 35 commands are defined here as clap
//! subcommands. Phase 0: each command prints "not yet implemented" until the
//! corresponding command module is wired up.

mod agents;
mod commands;
mod config;
mod coordinator;
mod db;
mod errors;
mod json;
mod logging;
mod merge;
mod process;
mod runtimes;
mod tui;
mod types;
mod watchdog;
mod worktree;

use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use logging::{brand_bold, muted, print_error, set_quiet};
use std::path::PathBuf;

const VERSION: &str = env!("GROVE_VERSION");

/// All top-level command names — used for fuzzy suggestion on unknown input.
const COMMANDS: &[&str] = &[
    "agents",
    "init",
    "sling",
    "spec",
    "prime",
    "stop",
    "status",
    "dashboard",
    "inspect",
    "clean",
    "doctor",
    "coordinator",
    "supervisor",
    "hooks",
    "monitor",
    "mail",
    "merge",
    "nudge",
    "group",
    "worktree",
    "log",
    "logs",
    "watch",
    "trace",
    "ecosystem",
    "feed",
    "errors",
    "replay",
    "run",
    "costs",
    "metrics",
    "eval",
    "update",
    "upgrade",
    "completions",
];

// ---------------------------------------------------------------------------
// Levenshtein distance (for typo suggestions)
// ---------------------------------------------------------------------------

fn edit_distance(a: &str, b: &str) -> usize {
    let m = a.len();
    let n = b.len();
    let mut dp = vec![0usize; (m + 1) * (n + 1)];
    let idx = |i: usize, j: usize| i * (n + 1) + j;
    for i in 0..=m {
        dp[idx(i, 0)] = i;
    }
    for j in 0..=n {
        dp[idx(0, j)] = j;
    }
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    for i in 1..=m {
        for j in 1..=n {
            let cost = usize::from(a_bytes[i - 1] != b_bytes[j - 1]);
            dp[idx(i, j)] = (dp[idx(i - 1, j)] + 1)
                .min(dp[idx(i, j - 1)] + 1)
                .min(dp[idx(i - 1, j - 1)] + cost);
        }
    }
    dp[idx(m, n)]
}

fn suggest_command(input: &str) -> Option<&'static str> {
    let mut best: Option<&'static str> = None;
    let mut best_dist = 3usize; // only suggest if distance ≤ 2
    for &cmd in COMMANDS {
        let dist = edit_distance(input, cmd);
        if dist < best_dist {
            best_dist = dist;
            best = Some(cmd);
        }
    }
    best
}

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "grove",
    about = "Multi-agent orchestration for AI coding agents",
    version = VERSION,
    disable_help_subcommand = true,
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Target project root (overrides auto-detection)
    #[arg(long, global = true)]
    project: Option<PathBuf>,

    /// Suppress non-error output
    #[arg(short, long, global = true)]
    quiet: bool,

    /// JSON output
    #[arg(long, global = true)]
    json: bool,

    /// Verbose output
    #[arg(long, global = true)]
    verbose: bool,

    /// Print command execution time to stderr
    #[arg(long, global = true)]
    timing: bool,
}

// ---------------------------------------------------------------------------
// Subcommand enum — all 35 commands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum Commands {
    /// List running agents and their status
    Agents(AgentsArgs),

    /// Initialize .overstory/ and bootstrap ecosystem tools
    Init(InitArgs),

    /// Spawn a worker agent
    Sling(SlingArgs),

    /// Manage task specifications
    Spec(SpecArgs),

    /// Load context for orchestrator/agent
    Prime(PrimeArgs),

    /// Terminate a running agent
    Stop(StopArgs),

    /// Show system status
    Status(StatusArgs),

    /// Interactive TUI dashboard
    Dashboard(PassthroughArgs),

    /// Inspect agent details
    Inspect(InspectArgs),

    /// Wipe runtime state (nuclear cleanup)
    Clean(CleanArgs),

    /// Check system health
    Doctor(DoctorArgs),

    /// Persistent coordinator event loop
    Coordinator(CoordinatorArgs),

    /// Agent supervisor daemon
    Supervisor(PassthroughArgs),

    /// Manage lifecycle hooks
    Hooks(HooksArgs),

    /// Monitor agents in real time
    Monitor(MonitorArgs),

    /// Mail system (send/check/list/read/reply)
    Mail(MailArgs),

    /// Merge agent branches into canonical
    Merge(MergeArgs),

    /// Send a text nudge to an agent
    Nudge(NudgeArgs),

    /// Group management
    Group(GroupArgs),

    /// Worktree management
    Worktree(WorktreeArgs),

    /// Session lifecycle hooks (session-start / session-end)
    Log(LogArgs),

    /// Query NDJSON logs across agents
    Logs(LogsArgs),

    /// Watch agents in real time
    Watch(WatchArgs),

    /// Chronological event timeline for agent or task
    Trace(TraceArgs),

    /// Manage ecosystem tools (mulch, seeds, canopy)
    Ecosystem(EcosystemArgs),

    /// Stream live event feed
    Feed(FeedArgs),

    /// Query error events
    Errors(ErrorsArgs),

    /// Replay agent sessions
    Replay(ReplayArgs),

    /// Manage runs (coordinator session groupings)
    Run(RunArgs),

    /// Show token costs and spending
    Costs(CostsArgs),

    /// Show session metrics
    Metrics(MetricsArgs),

    /// Run evaluations (coming soon)
    #[command(hide = true)]
    Eval(PassthroughArgs),

    /// Refresh .overstory/ managed files from embedded defaults
    Update(UpdateArgs),

    /// Upgrade grove to the latest version
    Upgrade(UpgradeArgs),

    /// Generate shell completion scripts
    Completions(CompletionsArgs),
}

// ---------------------------------------------------------------------------
// Per-command argument structs
// ---------------------------------------------------------------------------

/// Catch-all for commands whose subcommands are not yet implemented.
#[derive(Args)]
struct PassthroughArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Args)]
struct AgentsArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
    /// Filter by agent name
    #[arg(long)]
    agent: Option<String>,
    /// Filter by capability (builder|scout|reviewer|lead|merger)
    #[arg(long)]
    capability: Option<String>,
    /// Filter by state (working|booting|stalled|zombie|completed)
    #[arg(long)]
    state: Option<String>,
    /// Filter by run ID
    #[arg(long)]
    run: Option<String>,
    /// Compact single-line output
    #[arg(long)]
    compact: bool,
    /// Include completed and zombie agents (default: active only)
    #[arg(long)]
    all: bool,
    /// Watch mode (refresh every N seconds)
    #[arg(long)]
    watch: Option<u64>,
}

#[derive(Args)]
struct InitArgs {
    /// Reinitialize even if .overstory/ already exists
    #[arg(long)]
    force: bool,
    /// Accept all defaults without prompting
    #[arg(short, long)]
    yes: bool,
    /// Project name (skips auto-detection)
    #[arg(long)]
    name: Option<String>,
    /// Comma-separated list of ecosystem tools to bootstrap
    #[arg(long)]
    tools: Option<String>,
    /// Skip mulch bootstrap
    #[arg(long)]
    skip_mulch: bool,
    /// Skip seeds bootstrap
    #[arg(long)]
    skip_seeds: bool,
    /// Skip canopy bootstrap
    #[arg(long)]
    skip_canopy: bool,
    /// Skip CLAUDE.md onboarding step
    #[arg(long)]
    skip_onboard: bool,
    /// Output result as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct SlingArgs {
    /// Task ID to assign
    task_id: String,
    /// Agent type: builder | scout | reviewer | lead | merger
    #[arg(long, default_value = "builder")]
    capability: String,
    /// Unique agent name (auto-generated if omitted)
    #[arg(long)]
    name: Option<String>,
    /// Path to task spec file
    #[arg(long)]
    spec: Option<PathBuf>,
    /// Exclusive file scope (comma-separated)
    #[arg(long)]
    files: Option<String>,
    /// Parent agent for hierarchy tracking
    #[arg(long)]
    parent: Option<String>,
    /// Current hierarchy depth
    #[arg(long, default_value = "0")]
    depth: u32,
    /// Skip scout phase for lead agents
    #[arg(long)]
    skip_scout: bool,
    /// Skip task existence validation
    #[arg(long)]
    skip_task_check: bool,
    /// Bypass hierarchy validation
    #[arg(long)]
    force_hierarchy: bool,
    /// Max children per lead
    #[arg(long)]
    max_agents: Option<u32>,
    /// Skip review phase for lead agents
    #[arg(long)]
    skip_review: bool,
    /// Suppress parentHasScouts warning
    #[arg(long)]
    no_scout_check: bool,
    /// Per-lead max agents ceiling (injected into overlay)
    #[arg(long)]
    dispatch_max_agents: Option<u32>,
    /// Runtime adapter (default: config or claude)
    #[arg(long)]
    runtime: Option<String>,
    /// Base branch for worktree creation
    #[arg(long)]
    base_branch: Option<String>,
    /// Suppress directive rendering in overlay
    #[arg(long)]
    no_directives: bool,
    /// Spawn as headless child process (no tmux), overrides runtime default
    #[arg(long)]
    headless: bool,
    /// Output result as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct PrimeArgs {
    /// Prime for a specific agent
    #[arg(long)]
    agent: Option<String>,
    /// Output reduced context (for PreCompact hook)
    #[arg(long)]
    compact: bool,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct StopArgs {
    /// Name of the agent to stop
    agent_name: String,
    /// Force kill and force-delete branch
    #[arg(long)]
    force: bool,
    /// Remove the agent's worktree after stopping
    #[arg(long)]
    clean_worktree: bool,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct StatusArgs {
    /// Filter by agent name
    #[arg(long)]
    agent: Option<String>,
    /// Output as JSON
    #[arg(long)]
    json: bool,
    /// Compact single-line output per agent
    #[arg(long)]
    compact: bool,
    /// Filter by run ID
    #[arg(long)]
    run: Option<String>,
}

#[derive(Args)]
struct CleanArgs {
    /// Skip any confirmation prompt (for scripted use)
    #[arg(short, long)]
    force: bool,
    /// Wipe everything (nuclear option)
    #[arg(long)]
    all: bool,
    /// Delete mail.db
    #[arg(long)]
    mail: bool,
    /// Wipe sessions.db
    #[arg(long)]
    sessions: bool,
    /// Delete metrics.db
    #[arg(long)]
    metrics: bool,
    /// Remove all agent logs
    #[arg(long)]
    logs: bool,
    /// Remove all worktrees and kill tmux sessions
    #[arg(long)]
    worktrees: bool,
    /// Delete all overstory/* branch refs
    #[arg(long)]
    branches: bool,
    /// Remove agent identity files
    #[arg(long)]
    agents: bool,
    /// Remove task spec files
    #[arg(long)]
    specs: bool,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct DoctorArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
    /// Verbose output
    #[arg(long)]
    verbose: bool,
    /// Filter by category
    #[arg(long)]
    category: Option<String>,
}

#[derive(Args)]
struct MailArgs {
    #[command(subcommand)]
    command: MailSubcommand,
}

#[derive(Subcommand)]
enum MailSubcommand {
    /// List messages
    List(MailListArgs),
    /// Check unread messages for an agent
    Check(MailCheckArgs),
    /// Read a specific message
    Read(MailReadArgs),
    /// Send a message
    Send(MailSendArgs),
    /// Reply to a message
    Reply(MailReplyArgs),
    /// Purge messages
    Purge(MailPurgeArgs),
}

#[derive(Args)]
struct MailListArgs {
    /// Filter by sender
    #[arg(long)]
    from: Option<String>,
    /// Filter by recipient
    #[arg(long)]
    to: Option<String>,
    /// Filter by message type
    #[arg(long, name = "type")]
    message_type: Option<String>,
    /// Show only unread messages
    #[arg(long)]
    unread: bool,
    /// Limit number of results
    #[arg(long)]
    limit: Option<i64>,
}

#[derive(Args)]
struct MailCheckArgs {
    /// Agent name to check mail for
    #[arg(long)]
    agent: String,
    /// Output in inject format (for Claude Code hooks)
    #[arg(long)]
    inject: bool,
}

#[derive(Args)]
struct MailReadArgs {
    /// Message ID to read
    id: String,
}

#[derive(Args)]
struct MailSendArgs {
    /// Recipient agent name
    #[arg(long)]
    to: String,
    /// Message subject
    #[arg(long)]
    subject: String,
    /// Message body
    #[arg(long)]
    body: String,
    /// Message type (status, question, result, error, worker_done, merge_ready, merged, merge_failed, escalation, health_check, dispatch, assign)
    #[arg(long, name = "type", default_value = "status")]
    message_type: String,
    /// Priority level (low, normal, high, urgent)
    #[arg(long, default_value = "normal")]
    priority: String,
    /// Thread ID to associate with
    #[arg(long)]
    thread: Option<String>,
    /// Sender agent name (defaults to "operator")
    #[arg(long, alias = "from", default_value = "operator")]
    agent: String,
    /// Structured JSON payload
    #[arg(long)]
    payload: Option<String>,
}

#[derive(Args)]
struct MailReplyArgs {
    /// Message ID to reply to
    id: String,
    /// Reply body
    #[arg(long)]
    body: String,
    /// Sender agent name (defaults to "operator")
    #[arg(long, alias = "from", default_value = "operator")]
    agent: String,
}

#[derive(Args)]
struct MailPurgeArgs {
    /// Purge messages for specific agent
    #[arg(long)]
    agent: Option<String>,
    /// Purge all messages
    #[arg(long)]
    all: bool,
    /// Purge messages older than N days
    #[arg(long)]
    days: Option<u32>,
}

#[derive(Args)]
struct MergeArgs {
    /// Merge a specific branch
    #[arg(long)]
    branch: Option<String>,
    /// Merge all pending branches in the queue
    #[arg(long)]
    all: bool,
    /// Target branch to merge into
    #[arg(long)]
    into: Option<String>,
    /// Check for conflicts without actually merging
    #[arg(long)]
    dry_run: bool,
    /// Output results as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct NudgeArgs {
    /// Name of the agent to nudge
    agent_name: String,
    /// Custom nudge message
    #[arg(long)]
    message: Option<String>,
    /// Sender agent name (defaults to "operator")
    #[arg(long, default_value = "operator")]
    from: String,
    /// Bypass debounce window
    #[arg(long)]
    force: bool,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct GroupArgs {
    #[command(subcommand)]
    command: GroupSubcommand,
}

#[derive(Subcommand)]
enum GroupSubcommand {
    /// Create a new task group
    Create(GroupCreateArgs),
    /// Show progress for one or all groups
    Status(GroupStatusArgs),
    /// Add issues to a group
    Add(GroupAddArgs),
    /// Remove issues from a group
    Remove(GroupRemoveArgs),
    /// List all groups
    List(GroupListArgs),
}

#[derive(Args)]
struct GroupCreateArgs {
    /// Group name
    name: String,
    /// Issue IDs to include
    ids: Vec<String>,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct GroupStatusArgs {
    /// Group ID (optional, shows all if omitted)
    group_id: Option<String>,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct GroupAddArgs {
    /// Group ID
    group_id: String,
    /// Issue IDs to add
    ids: Vec<String>,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct GroupRemoveArgs {
    /// Group ID
    group_id: String,
    /// Issue IDs to remove
    ids: Vec<String>,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct GroupListArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct WorktreeArgs {
    #[command(subcommand)]
    command: WorktreeSubcommand,
}

#[derive(Subcommand)]
enum WorktreeSubcommand {
    /// List worktrees with agent status
    List(WorktreeListArgs),
    /// Remove completed worktrees
    Clean(WorktreeCleanArgs),
}

#[derive(Args)]
struct WorktreeListArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct WorktreeCleanArgs {
    /// Only finished agents (default)
    #[arg(long)]
    completed: bool,
    /// Force remove all worktrees
    #[arg(long)]
    all: bool,
    /// Delete even if branches are unmerged
    #[arg(long)]
    force: bool,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct RunArgs {
    #[command(subcommand)]
    command: Option<RunSubcommand>,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
enum RunSubcommand {
    /// List recent runs
    List(RunListArgs),
    /// Show run details (agents, duration)
    Show(RunShowArgs),
    /// Mark current run as completed
    Complete(RunCompleteArgs),
}

#[derive(Args)]
struct RunListArgs {
    /// Number of recent runs to show
    #[arg(long, default_value = "10")]
    last: u32,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct RunShowArgs {
    /// Run ID
    id: String,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct RunCompleteArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct SpecArgs {
    #[command(subcommand)]
    command: SpecSubcommand,
}

#[derive(Subcommand)]
enum SpecSubcommand {
    /// Write a task specification file
    Write(SpecWriteArgs),
}

#[derive(Args)]
struct SpecWriteArgs {
    /// Task ID
    task_id: String,
    /// Spec body content
    #[arg(long)]
    body: Option<String>,
    /// Read spec from file
    #[arg(long)]
    file: Option<PathBuf>,
    /// Agent name for tracking
    #[arg(long)]
    agent: Option<String>,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct HooksArgs {
    #[command(subcommand)]
    command: HooksSubcommand,
}

#[derive(Subcommand)]
enum HooksSubcommand {
    /// Install orchestrator hooks
    Install(HooksInstallArgs),
    /// Uninstall orchestrator hooks
    Uninstall(HooksUninstallArgs),
    /// Show hooks status
    Status(HooksStatusArgs),
}

#[derive(Args)]
struct HooksInstallArgs {
    /// Overwrite existing hooks
    #[arg(long)]
    force: bool,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct HooksUninstallArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct HooksStatusArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct LogArgs {
    #[command(subcommand)]
    command: LogSubcommand,
}

#[derive(Subcommand)]
enum LogSubcommand {
    /// Mark session as working (called by SessionStart hook)
    SessionStart(LogSessionStartArgs),
    /// Mark session as completed (called by Stop hook)
    SessionEnd(LogSessionEndArgs),
}

#[derive(Args)]
struct LogSessionStartArgs {
    /// Agent name
    #[arg(long)]
    agent: String,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct LogSessionEndArgs {
    /// Agent name
    #[arg(long)]
    agent: String,
    /// Exit code of the session
    #[arg(long)]
    exit_code: Option<i32>,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct CostsArgs {
    /// Filter by agent name
    #[arg(long)]
    agent: Option<String>,
    /// Filter by run ID
    #[arg(long)]
    run: Option<String>,
    /// Show latest token snapshots (live cost tracking)
    #[arg(long)]
    live: bool,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct FeedArgs {
    /// Follow mode — stream new events as they arrive
    #[arg(long)]
    follow: bool,
    /// Filter by agent name
    #[arg(long)]
    agent: Option<String>,
    /// Filter by event type
    #[arg(long, name = "type")]
    event_type: Option<String>,
    /// Max number of events to show
    #[arg(long)]
    limit: Option<usize>,
}

#[derive(Args)]
struct LogsArgs {
    /// Filter by agent name
    #[arg(long)]
    agent: Option<String>,
    /// Filter by log level (debug|info|warn|error)
    #[arg(long)]
    level: Option<String>,
    /// Start time (ISO8601 or relative like "1h")
    #[arg(long)]
    since: Option<String>,
    /// End time (ISO8601)
    #[arg(long)]
    until: Option<String>,
    /// Max number of events to show
    #[arg(long)]
    limit: Option<i64>,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct ReplayArgs {
    /// Filter by run ID
    #[arg(long)]
    run: Option<String>,
    /// Filter by agent name(s)
    #[arg(long = "agent")]
    agents: Vec<String>,
    /// Start time filter
    #[arg(long)]
    since: Option<String>,
    /// End time filter
    #[arg(long)]
    until: Option<String>,
    /// Max number of events to show
    #[arg(long)]
    limit: Option<i64>,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct MetricsArgs {
    /// Number of recent sessions to show
    #[arg(long)]
    last: Option<i64>,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct MonitorArgs {
    #[command(subcommand)]
    command: MonitorSubcommand,
}

#[derive(Subcommand)]
enum MonitorSubcommand {
    /// Start the monitor daemon
    Start(MonitorStartArgs),
    /// Stop the monitor daemon
    Stop(MonitorStopArgs),
    /// Show monitor status
    Status(MonitorStatusArgs),
}

#[derive(Args)]
struct MonitorStartArgs {
    /// Run in foreground (used internally)
    #[arg(long, hide = true)]
    foreground: bool,
    /// Start in background without attaching to the terminal
    #[arg(long)]
    no_attach: bool,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct MonitorStopArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct MonitorStatusArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct WatchArgs {
    #[command(subcommand)]
    command: Option<WatchSubcommand>,
    /// Poll interval in milliseconds
    #[arg(long)]
    interval: Option<u64>,
    /// Run as background daemon
    #[arg(long)]
    background: bool,
    /// Run in foreground (used internally by daemon)
    #[arg(long, hide = true)]
    foreground: bool,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
enum WatchSubcommand {
    /// Stop the background watchdog daemon
    Stop(WatchStopArgs),
    /// Show watchdog daemon status
    Status(WatchStatusArgs),
}

#[derive(Args)]
struct WatchStopArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct WatchStatusArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct EcosystemArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct ErrorsArgs {
    /// Filter by agent name
    #[arg(long)]
    agent: Option<String>,
    /// Max number of agent groups to show
    #[arg(long)]
    limit: Option<usize>,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct CoordinatorArgs {
    #[command(subcommand)]
    command: CoordinatorSubcommand,
}

#[derive(Subcommand)]
enum CoordinatorSubcommand {
    /// Start the coordinator event loop
    Start(CoordinatorStartArgs),
    /// Stop the coordinator
    Stop(CoordinatorStopArgs),
    /// Show coordinator status
    Status(CoordinatorStatusArgs),
    /// Send a message to the coordinator's mailbox
    Send(CoordinatorSendArgs),
    /// Tail the coordinator log file
    Logs(CoordinatorLogsArgs),
}

#[derive(Args)]
struct CoordinatorStartArgs {
    /// Start in background (no-op: coordinator always runs as a daemon)
    #[arg(long)]
    no_attach: bool,
    /// Coordination profile (delivery | co-creation)
    #[arg(long)]
    profile: Option<String>,
    /// Run the event loop in the foreground (used internally by tmux)
    #[arg(long, hide = true)]
    foreground: bool,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct InspectArgs {
    /// Agent name to inspect
    agent_name: String,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct CoordinatorStopArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct TraceArgs {
    /// Agent name or task ID to trace
    subject: String,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct CoordinatorStatusArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct CoordinatorSendArgs {
    /// Message subject
    #[arg(long)]
    subject: String,
    /// Message body
    #[arg(long)]
    body: String,
    /// Sender agent name
    #[arg(long, default_value = "operator")]
    from: String,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct CoordinatorLogsArgs {
    /// Follow log output (poll for new content)
    #[arg(long, short = 'f')]
    follow: bool,
    /// Number of lines to show from the end
    #[arg(long, default_value = "50")]
    lines: usize,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct UpdateArgs {
    /// Refresh agent definition files only
    #[arg(long)]
    agents: bool,
    /// Refresh agent-manifest.json only
    #[arg(long)]
    manifest: bool,
    /// Refresh hooks.json only
    #[arg(long)]
    hooks: bool,
    /// Show what would change without writing
    #[arg(long)]
    dry_run: bool,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct UpgradeArgs {
    /// Check for updates without installing
    #[arg(long)]
    check: bool,
    /// Upgrade all os-eco ecosystem tools as well
    #[arg(long)]
    all: bool,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct CompletionsArgs {
    /// Shell to generate completions for (bash, zsh, fish, powershell, elvish)
    shell: Shell,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    // Handle `--version --json` before clap processes flags (matches TS behavior).
    let raw_args: Vec<String> = std::env::args().collect();
    let has_version = raw_args.iter().any(|a| a == "-v" || a == "--version");
    let has_json = raw_args.iter().any(|a| a == "--json");
    if has_version && has_json {
        let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
        println!(
            "{}",
            serde_json::json!({
                "name": "grove",
                "version": VERSION,
                "runtime": "cargo",
                "platform": platform,
            })
        );
        std::process::exit(0);
    }

    // Check for unknown subcommand before handing off to clap, so we can
    // suggest the closest known command via Levenshtein distance.
    if let Some(first_arg) = raw_args.get(1) {
        if !first_arg.starts_with('-') && !COMMANDS.contains(&first_arg.as_str()) {
            let unknown = first_arg.as_str();
            eprintln!("Unknown command: {unknown}");
            if let Some(suggestion) = suggest_command(unknown) {
                eprintln!("Did you mean '{suggestion}'?");
            }
            eprintln!("Run 'grove --help' for usage.");
            std::process::exit(1);
        }
    }

    let cli = Cli::parse();

    if cli.quiet {
        set_quiet(true);
    }

    let start = if cli.timing {
        Some(std::time::Instant::now())
    } else {
        None
    };

    let result = run_command(cli.command, cli.json, cli.verbose, cli.project.as_deref());

    if let Some(t) = start {
        let elapsed = t.elapsed();
        let formatted = if elapsed.as_millis() < 1000 {
            format!("{}ms", elapsed.as_millis())
        } else {
            format!("{:.2}s", elapsed.as_secs_f64())
        };
        eprintln!("{}", muted(&format!("Done in {formatted}")));
    }

    if let Err(e) = result {
        print_error(&e, None);
        std::process::exit(1);
    }
}

fn run_command(
    cmd: Commands,
    json: bool,
    _verbose: bool,
    project: Option<&std::path::Path>,
) -> Result<(), String> {
    match cmd {
        Commands::Agents(args) => commands::agents::execute_discover(
            args.capability,
            args.all,
            args.json || json,
            project,
        ),
        Commands::Init(args) => commands::init::execute(commands::init::InitOptions {
            name: args.name,
            yes: args.yes,
            force: args.force,
            tools: args.tools,
            skip_mulch: args.skip_mulch,
            skip_seeds: args.skip_seeds,
            skip_canopy: args.skip_canopy,
            skip_onboard: args.skip_onboard,
            json: args.json || json,
            project_override: project,
        }),
        Commands::Sling(args) => commands::sling::execute(commands::sling::SlingOptions {
            task_id: &args.task_id,
            capability: &args.capability,
            name: args.name.as_deref(),
            spec: args.spec.as_deref(),
            files: args.files.as_deref(),
            parent: args.parent.as_deref(),
            depth: args.depth,
            _skip_scout: args.skip_scout,
            skip_task_check: args.skip_task_check,
            force_hierarchy: args.force_hierarchy,
            max_agents: args.max_agents,
            skip_review: args.skip_review,
            _no_scout_check: args.no_scout_check,
            dispatch_max_agents: args.dispatch_max_agents,
            runtime: args.runtime.as_deref(),
            base_branch: args.base_branch.as_deref(),
            no_directives: args.no_directives,
            headless: args.headless,
            json: args.json || json,
            project_override: project,
        }),
        Commands::Spec(args) => match args.command {
            SpecSubcommand::Write(a) => commands::spec::execute_write(
                &a.task_id,
                a.body,
                a.file.as_deref(),
                a.agent,
                a.json || json,
                project,
            ),
        },
        Commands::Prime(args) => commands::prime::execute(
            args.agent,
            args.compact,
            args.json || json,
            project,
        ),
        Commands::Stop(args) => commands::stop::execute(
            &args.agent_name,
            args.force,
            args.clean_worktree,
            args.json || json,
            project,
        ),
        Commands::Status(args) => commands::status::execute(
            args.agent,
            args.run,
            args.compact,
            args.json || json,
            project,
        ),
        Commands::Dashboard(_) => {
            let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
            let project_root = if let Some(p) = project {
                p.to_string_lossy().to_string()
            } else {
                config::resolve_project_root(&cwd, None)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .to_string()
            };
            tui::launch_dashboard(&project_root).map_err(|e| e.to_string())
        }
        Commands::Inspect(args) => commands::inspect::execute(
            &args.agent_name,
            args.json || json,
            project,
        ),
        Commands::Clean(args) => commands::clean::execute(
            args.force,
            args.all,
            args.mail,
            args.sessions,
            args.metrics,
            args.logs,
            args.worktrees,
            args.branches,
            args.agents,
            args.specs,
            args.json || json,
            project,
        ),
        Commands::Doctor(args) => {
            let cmd_json = args.json || json;
            commands::doctor::execute(cmd_json, args.verbose, args.category)
        }
        Commands::Coordinator(args) => match args.command {
            CoordinatorSubcommand::Start(a) => commands::coordinator::execute_start(
                a.no_attach,
                a.profile.as_deref(),
                a.foreground,
                a.json || json,
                project,
            ),
            CoordinatorSubcommand::Stop(a) => {
                commands::coordinator::execute_stop(a.json || json, project)
            }
            CoordinatorSubcommand::Status(a) => {
                commands::coordinator::execute_status(a.json || json, project)
            }
            CoordinatorSubcommand::Send(a) => commands::coordinator::execute_send(
                &a.subject,
                &a.body,
                &a.from,
                a.json || json,
                project,
            ),
            CoordinatorSubcommand::Logs(a) => commands::coordinator::execute_logs(
                a.follow,
                a.lines,
                a.json || json,
                project,
            ),
        },
        Commands::Supervisor(_) => {
            println!("grove supervisor: deprecated. Use `grove coordinator` instead.");
            Ok(())
        }
        Commands::Hooks(args) => match args.command {
            HooksSubcommand::Install(a) => commands::hooks::execute_install(
                a.force,
                a.json || json,
                project,
            ),
            HooksSubcommand::Uninstall(a) => commands::hooks::execute_uninstall(a.json || json, project),
            HooksSubcommand::Status(a) => commands::hooks::execute_status(a.json || json, project),
        },
        Commands::Monitor(args) => match args.command {
            MonitorSubcommand::Start(a) => commands::monitor::execute_start(
                a.foreground,
                a.json || json,
                project,
            ),
            MonitorSubcommand::Stop(a) => commands::monitor::execute_stop(a.json || json, project),
            MonitorSubcommand::Status(a) => commands::monitor::execute_status(a.json || json, project),
        },
        Commands::Mail(args) => match args.command {
            MailSubcommand::List(a) => {
                commands::mail::execute_list(a.from, a.to, a.message_type, a.unread, a.limit, json)
            }
            MailSubcommand::Check(a) => commands::mail::execute_check(&a.agent, a.inject, json),
            MailSubcommand::Read(a) => commands::mail::execute_read(&a.id, json),
            MailSubcommand::Send(a) => commands::mail::execute_send(
                &a.to, &a.subject, &a.body, &a.message_type, &a.priority,
                a.thread.as_deref(), &a.agent, a.payload.as_deref(), json,
            ),
            MailSubcommand::Reply(a) => commands::mail::execute_reply(&a.id, &a.body, &a.agent, json),
            MailSubcommand::Purge(a) => commands::mail::execute_purge(a.agent.as_deref(), a.all, a.days, json),
        },
        Commands::Merge(args) => commands::merge::execute(
            args.branch,
            args.all,
            args.into,
            args.dry_run,
            args.json || json,
            project,
        ),
        Commands::Nudge(args) => commands::nudge::execute(
            &args.agent_name,
            args.message.as_deref(),
            &args.from,
            args.force,
            args.json || json,
            project,
        ),
        Commands::Group(args) => match args.command {
            GroupSubcommand::Create(a) => commands::group::execute_create(
                &a.name,
                a.ids,
                a.json || json,
                project,
            ),
            GroupSubcommand::Status(a) => commands::group::execute_status(
                a.group_id,
                a.json || json,
                project,
            ),
            GroupSubcommand::Add(a) => commands::group::execute_add(
                &a.group_id,
                a.ids,
                a.json || json,
                project,
            ),
            GroupSubcommand::Remove(a) => commands::group::execute_remove(
                &a.group_id,
                a.ids,
                a.json || json,
                project,
            ),
            GroupSubcommand::List(a) => commands::group::execute_list(a.json || json, project),
        },
        Commands::Worktree(args) => match args.command {
            WorktreeSubcommand::List(a) => commands::worktree_cmd::execute_list(a.json || json, project),
            WorktreeSubcommand::Clean(a) => {
                let all = a.all;
                commands::worktree_cmd::execute_clean(
                    all,
                    a.force,
                    a.completed || !all,
                    a.json || json,
                    project,
                )
            }
        },
        Commands::Log(args) => match args.command {
            LogSubcommand::SessionStart(a) => commands::log::execute_session_start(
                &a.agent,
                a.json || json,
                project,
            ),
            LogSubcommand::SessionEnd(a) => commands::log::execute_session_end(
                &a.agent,
                a.exit_code,
                a.json || json,
                project,
            ),
        },
        Commands::Logs(args) => commands::logs::execute(
            args.agent,
            args.level,
            args.since,
            args.until,
            args.limit,
            args.json || json,
            project,
        ),
        Commands::Watch(args) => match args.command {
            None => commands::watch_cmd::execute(
                args.interval,
                args.background,
                args.foreground,
                args.json || json,
                project,
            ),
            Some(WatchSubcommand::Stop(a)) => {
                commands::watch_cmd::execute_stop(a.json || json, project)
            }
            Some(WatchSubcommand::Status(a)) => {
                commands::watch_cmd::execute_status(a.json || json, project)
            }
        },
        Commands::Trace(args) => commands::trace::execute(
            &args.subject,
            args.json || json,
            project,
        ),
        Commands::Ecosystem(args) => commands::ecosystem::execute(args.json || json, project),
        Commands::Feed(args) => commands::feed::execute(
            args.follow,
            args.agent,
            args.event_type,
            args.limit,
            json,
            project,
        ),
        Commands::Errors(args) => commands::errors::execute(
            args.agent,
            args.limit,
            args.json || json,
            project,
        ),
        Commands::Replay(args) => commands::replay::execute(
            args.run,
            args.agents,
            args.since,
            args.until,
            args.limit,
            args.json || json,
            project,
        ),
        Commands::Run(args) => {
            let cmd_json = args.json || json;
            match args.command {
                None => commands::run::execute_current(cmd_json, project),
                Some(RunSubcommand::List(a)) => commands::run::execute_list(a.last, a.json || cmd_json, project),
                Some(RunSubcommand::Show(a)) => commands::run::execute_show(&a.id, a.json || cmd_json, project),
                Some(RunSubcommand::Complete(a)) => commands::run::execute_complete(a.json || cmd_json, project),
            }
        }
        Commands::Costs(args) => commands::costs::execute(
            args.agent,
            args.run,
            args.live,
            args.json || json,
            project,
        ),
        Commands::Metrics(args) => commands::metrics_cmd::execute(
            args.last,
            args.json || json,
            project,
        ),
        Commands::Eval(_) => not_yet_implemented("eval", json),
        Commands::Update(args) => commands::update_cmd::execute(
            commands::update_cmd::UpdateOptions {
                agents: args.agents,
                manifest: args.manifest,
                hooks: args.hooks,
                dry_run: args.dry_run,
                json: args.json || json,
            },
            project,
        ),
        Commands::Upgrade(args) => commands::upgrade_cmd::execute(
            commands::upgrade_cmd::UpgradeOptions {
                check: args.check,
                all: args.all,
                json: args.json || json,
            },
        ),
        Commands::Completions(args) => {
            let mut cmd = Cli::command();
            commands::completions::execute(args.shell, &mut cmd)
        }
    }
}

fn not_yet_implemented(command: &str, json: bool) -> Result<(), String> {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "command": command,
                "error": "not yet implemented",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            })
        );
    } else {
        println!("{} {}: not yet implemented", brand_bold("grove"), command);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_distance_same() {
        assert_eq!(edit_distance("status", "status"), 0);
    }

    #[test]
    fn test_edit_distance_one_off() {
        assert_eq!(edit_distance("statsu", "status"), 2);
    }

    #[test]
    fn test_suggest_command_close_match() {
        assert_eq!(suggest_command("statu"), Some("status"));
    }

    #[test]
    fn test_suggest_command_no_match() {
        assert!(suggest_command("xxxxxxxxxx").is_none());
    }

    #[test]
    fn test_not_yet_implemented_plain() {
        // smoke test — just verify it doesn't panic
        assert!(not_yet_implemented("status", false).is_ok());
    }

    #[test]
    fn test_not_yet_implemented_json() {
        assert!(not_yet_implemented("status", true).is_ok());
    }
}
