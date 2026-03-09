//! Grove CLI — multi-agent orchestration for AI coding agents.
//!
//! Port of `reference/index.ts`. All 35 commands are defined here as clap
//! subcommands. Phase 0: each command prints "not yet implemented" until the
//! corresponding command module is wired up.

mod commands;
mod config;
mod db;
mod errors;
mod json;
mod logging;
mod types;

use clap::{Args, Parser, Subcommand};
use logging::{brand_bold, muted, print_error, set_quiet};
use std::path::PathBuf;

const VERSION: &str = env!("CARGO_PKG_VERSION");

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
    Spec(PassthroughArgs),

    /// Load context for orchestrator/agent
    Prime(PrimeArgs),

    /// Terminate a running agent
    Stop(StopArgs),

    /// Show system status
    Status(StatusArgs),

    /// Interactive TUI dashboard
    Dashboard(PassthroughArgs),

    /// Inspect agent details
    Inspect(PassthroughArgs),

    /// Wipe runtime state (nuclear cleanup)
    Clean(CleanArgs),

    /// Check system health
    Doctor(DoctorArgs),

    /// Persistent coordinator event loop
    Coordinator(PassthroughArgs),

    /// Agent supervisor daemon
    Supervisor(PassthroughArgs),

    /// Manage lifecycle hooks
    Hooks(PassthroughArgs),

    /// Monitor agents in real time
    Monitor(PassthroughArgs),

    /// Mail system (send/check/list/read/reply)
    Mail(MailArgs),

    /// Merge agent branches into canonical
    Merge(MergeArgs),

    /// Send a text nudge to an agent
    Nudge(PassthroughArgs),

    /// Group management
    Group(PassthroughArgs),

    /// Worktree management
    Worktree(PassthroughArgs),

    /// Query agent logs
    Log(PassthroughArgs),

    /// Query NDJSON logs across agents
    Logs(PassthroughArgs),

    /// Watch agents in real time
    Watch(PassthroughArgs),

    /// Chronological event timeline for agent or task
    Trace(PassthroughArgs),

    /// Manage ecosystem tools (mulch, seeds, canopy)
    Ecosystem(PassthroughArgs),

    /// Stream live event feed
    Feed(PassthroughArgs),

    /// Query error events
    Errors(PassthroughArgs),

    /// Replay agent sessions
    Replay(PassthroughArgs),

    /// Run a task end-to-end
    Run(PassthroughArgs),

    /// Show token costs and spending
    Costs(CostsArgs),

    /// Show session metrics
    Metrics(PassthroughArgs),

    /// Run evaluations
    Eval(PassthroughArgs),

    /// Update grove
    Update(PassthroughArgs),

    /// Upgrade dependencies
    Upgrade(PassthroughArgs),

    /// Generate shell completions
    Completions(PassthroughArgs),
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
    #[arg(long, default_value = "operator")]
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
    #[arg(long, default_value = "operator")]
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
        Commands::Agents(_) => not_yet_implemented("agents", json),
        Commands::Init(_) => not_yet_implemented("init", json),
        Commands::Sling(_) => not_yet_implemented("sling", json),
        Commands::Spec(_) => not_yet_implemented("spec", json),
        Commands::Prime(_) => not_yet_implemented("prime", json),
        Commands::Stop(_) => not_yet_implemented("stop", json),
        Commands::Status(args) => commands::status::execute(
            args.agent,
            args.run,
            args.compact,
            args.json || json,
            project,
        ),
        Commands::Dashboard(_) => not_yet_implemented("dashboard", json),
        Commands::Inspect(_) => not_yet_implemented("inspect", json),
        Commands::Clean(_) => not_yet_implemented("clean", json),
        Commands::Doctor(args) => {
            let cmd_json = args.json || json;
            commands::doctor::execute(cmd_json, args.verbose, args.category)
        }
        Commands::Coordinator(_) => not_yet_implemented("coordinator", json),
        Commands::Supervisor(_) => not_yet_implemented("supervisor", json),
        Commands::Hooks(_) => not_yet_implemented("hooks", json),
        Commands::Monitor(_) => not_yet_implemented("monitor", json),
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
        Commands::Merge(_) => not_yet_implemented("merge", json),
        Commands::Nudge(_) => not_yet_implemented("nudge", json),
        Commands::Group(_) => not_yet_implemented("group", json),
        Commands::Worktree(_) => not_yet_implemented("worktree", json),
        Commands::Log(_) => not_yet_implemented("log", json),
        Commands::Logs(_) => not_yet_implemented("logs", json),
        Commands::Watch(_) => not_yet_implemented("watch", json),
        Commands::Trace(_) => not_yet_implemented("trace", json),
        Commands::Ecosystem(_) => not_yet_implemented("ecosystem", json),
        Commands::Feed(_) => not_yet_implemented("feed", json),
        Commands::Errors(_) => not_yet_implemented("errors", json),
        Commands::Replay(_) => not_yet_implemented("replay", json),
        Commands::Run(_) => not_yet_implemented("run", json),
        Commands::Costs(args) => commands::costs::execute(
            args.agent,
            args.run,
            args.live,
            args.json || json,
            project,
        ),
        Commands::Metrics(_) => not_yet_implemented("metrics", json),
        Commands::Eval(_) => not_yet_implemented("eval", json),
        Commands::Update(_) => not_yet_implemented("update", json),
        Commands::Upgrade(_) => not_yet_implemented("upgrade", json),
        Commands::Completions(_) => not_yet_implemented("completions", json),
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
