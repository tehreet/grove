//! `grove sling <task-id>` — spawn a worker agent.
//!
//! Orchestrates the full agent spawn pipeline:
//! 1. Load config + validate
//! 2. Load manifest + validate capability
//! 3. Resolve/create run_id
//! 4. Check limits (depth, concurrency, task lock)
//! 5. Create git worktree + branch
//! 6. Write overlay (CLAUDE.md) to worktree
//! 7. Deploy hooks config via runtime adapter
//! 8. Send auto-dispatch mail
//! 9. Claim task in tracker
//! 10. Create/load agent identity
//! 11. Spawn (headless stdin/stdout or tmux)
//! 12. Record session in sessions.db
//! 13. Output result

use std::fs;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::agents::manifest::load_manifest_from_project;
use crate::agents::overlay::write_overlay;
use crate::config::load_config;
use crate::db::mail::MailStore;
use crate::db::sessions::{RunStore, SessionStore};
use crate::json::json_output;
use crate::logging::print_success;
use crate::runtimes::registry::get_runtime;
use crate::runtimes::{HooksDef, SpawnOpts};
use crate::types::{
    AgentIdentity, AgentSession, AgentState, InsertMailMessage, InsertRun, MailMessageType,
    MailPriority, OverlayConfig, ResolvedModel, RunStatus,
};
use crate::worktree::git::{create_worktree, rollback_worktree};
use crate::worktree::tmux;

// ---------------------------------------------------------------------------
// Pure functions (testable)
// ---------------------------------------------------------------------------

/// Generate a unique agent name from capability and task ID.
/// Appends -2, -3, ... if the base name is taken.
pub fn generate_agent_name(capability: &str, task_id: &str, taken: &[String]) -> String {
    let base = format!("{capability}-{task_id}");
    if !taken.contains(&base) {
        return base;
    }
    for i in 2..=100 {
        let candidate = format!("{base}-{i}");
        if !taken.contains(&candidate) {
            return candidate;
        }
    }
    format!("{base}-{}", chrono::Utc::now().timestamp_millis())
}

/// Calculate how many milliseconds to wait before spawning.
/// Returns 0 if no delay is needed.
pub fn calculate_stagger_delay(
    stagger_ms: u64,
    active_sessions: &[AgentSession],
    now_ms: u64,
) -> u64 {
    if stagger_ms == 0 || active_sessions.is_empty() {
        return 0;
    }
    let most_recent = active_sessions
        .iter()
        .filter_map(|s| {
            chrono::DateTime::parse_from_rfc3339(&s.started_at)
                .ok()
                .map(|dt| dt.timestamp_millis() as u64)
        })
        .max()
        .unwrap_or(0);
    let elapsed = now_ms.saturating_sub(most_recent);
    stagger_ms.saturating_sub(elapsed)
}

/// Build the auto-dispatch mail body for a newly slung agent.
pub fn build_dispatch_body(
    task_id: &str,
    capability: &str,
    spec_path: Option<&str>,
    instruction_path: &str,
) -> String {
    let spec_line = spec_path
        .map(|p| format!("Spec file: {p}"))
        .unwrap_or_else(|| "No spec file provided. Check your overlay for task details.".to_string());
    format!(
        "You have been assigned task {task_id} as a {capability} agent. {spec_line} Read your overlay at {instruction_path} and begin immediately."
    )
}

/// Build the startup beacon sent to the agent's stdin (headless) or tmux (interactive).
pub fn build_beacon(
    agent_name: &str,
    capability: &str,
    task_id: &str,
    parent_agent: Option<&str>,
    depth: u32,
    instruction_path: &str,
) -> String {
    let ts = chrono::Utc::now().to_rfc3339();
    let parent = parent_agent.unwrap_or("none");
    format!(
        "[OVERSTORY] {agent_name} ({capability}) {ts} task:{task_id} — Depth: {depth} | Parent: {parent} — Startup: read {instruction_path}, run mulch prime, check mail (ov mail check --agent {agent_name}), then begin task {task_id}"
    )
}

/// Validate hierarchy: coordinator (no --parent) can only spawn lead/scout/builder.
pub fn validate_hierarchy(
    parent_agent: Option<&str>,
    capability: &str,
    force: bool,
) -> Result<(), String> {
    if force {
        return Ok(());
    }
    let direct_spawn = ["lead", "scout", "builder"];
    if parent_agent.is_none() && !direct_spawn.contains(&capability) {
        return Err(format!(
            "Coordinator cannot spawn \"{capability}\" directly. Only lead, scout, and builder \
             are allowed without --parent. Pass --force-hierarchy to bypass."
        ));
    }
    Ok(())
}

/// Get the current git branch name for a repo root.
pub fn get_current_branch(repo_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_root)
        .output()
        .ok()?;
    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if branch.is_empty() || branch == "HEAD" {
            None
        } else {
            Some(branch)
        }
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Options struct
// ---------------------------------------------------------------------------

pub struct SlingOptions<'a> {
    pub task_id: &'a str,
    pub capability: &'a str,
    pub name: Option<&'a str>,
    pub spec: Option<&'a Path>,
    pub files: Option<&'a str>,
    pub parent: Option<&'a str>,
    pub depth: u32,
    pub _skip_scout: bool,
    pub skip_task_check: bool,
    pub force_hierarchy: bool,
    pub max_agents: Option<u32>,
    pub skip_review: bool,
    pub _no_scout_check: bool,
    pub dispatch_max_agents: Option<u32>,
    pub runtime: Option<&'a str>,
    pub base_branch: Option<&'a str>,
    pub no_directives: bool,
    /// Force headless spawn (no tmux), overriding runtime default
    pub headless: bool,
    pub json: bool,
    pub project_override: Option<&'a Path>,
}

// ---------------------------------------------------------------------------
// Main execute
// ---------------------------------------------------------------------------

pub fn execute(opts: SlingOptions<'_>) -> Result<(), String> {
    let task_id = opts.task_id.trim();
    if task_id.is_empty() {
        return Err("Task ID is required: grove sling <task-id>".to_string());
    }

    let capability = opts.capability;

    // Resolve spec path (must exist if provided)
    let spec_path: Option<PathBuf> = if let Some(sp) = opts.spec {
        let abs = fs::canonicalize(sp).unwrap_or_else(|_| sp.to_path_buf());
        if !abs.exists() {
            return Err(format!("Spec file not found: {}", sp.display()));
        }
        Some(abs)
    } else {
        None
    };

    // File scope
    let file_scope: Vec<String> = opts
        .files
        .map(|f| {
            f.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Load config
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, opts.project_override).map_err(|e| e.to_string())?;
    let root = PathBuf::from(&config.project.root);
    let overstory_dir = root.join(".overstory");

    // Depth limit
    if opts.depth > config.agents.max_depth {
        return Err(format!(
            "Depth limit exceeded: depth {} > maxDepth {}",
            opts.depth, config.agents.max_depth
        ));
    }

    // Load manifest + validate capability BEFORE hierarchy check so unknown
    // capability gives a clear error instead of a confusing hierarchy error.
    let manifest =
        load_manifest_from_project(&root, &config.agents.manifest_path)
            .map_err(|e| e.to_string())?;
    let agent_def = manifest.agents.get(capability).ok_or_else(|| {
        format!(
            "Unknown capability \"{capability}\". Available: {}",
            manifest
                .agents
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;

    // Validate hierarchy after capability so callers get the right error first
    validate_hierarchy(opts.parent, capability, opts.force_hierarchy)?;

    // Resolve/create run_id
    let sessions_db = overstory_dir.join("sessions.db");
    let sessions_db_str = sessions_db.to_string_lossy().to_string();
    let current_run_path = overstory_dir.join("current-run.txt");

    let run_id = if current_run_path.exists() {
        fs::read_to_string(&current_run_path)
            .map(|s| s.trim().to_string())
            .map_err(|e| format!("Failed to read current-run.txt: {e}"))?
    } else {
        let new_id = format!(
            "run-{}",
            chrono::Utc::now()
                .to_rfc3339()
                .replace([':', '.'], "-")
        );
        let run_store = RunStore::new(&sessions_db_str).map_err(|e| e.to_string())?;
        run_store
            .create_run(&InsertRun {
                id: new_id.clone(),
                started_at: chrono::Utc::now().to_rfc3339(),
                coordinator_session_id: None,
                status: RunStatus::Active,
                agent_count: None,
            })
            .map_err(|e| e.to_string())?;
        fs::write(&current_run_path, &new_id)
            .map_err(|e| format!("Failed to write current-run.txt: {e}"))?;
        new_id
    };

    // Open session store
    let session_store = SessionStore::new(&sessions_db_str).map_err(|e| e.to_string())?;

    // Per-run session limit
    if config.agents.max_sessions_per_run > 0 {
        let run_store = RunStore::new(&sessions_db_str).map_err(|e| e.to_string())?;
        if let Some(run) = run_store.get_run(&run_id).map_err(|e| e.to_string())? {
            if run.agent_count >= config.agents.max_sessions_per_run {
                return Err(format!(
                    "Run session limit reached: {}/{} agents in run \"{}\"",
                    run.agent_count, config.agents.max_sessions_per_run, run_id
                ));
            }
        }
    }

    // Active sessions + concurrency limit
    let active_sessions = session_store.get_active().map_err(|e| e.to_string())?;
    if active_sessions.len() as u32 >= config.agents.max_concurrent {
        return Err(format!(
            "Max concurrent agent limit reached: {}/{}",
            active_sessions.len(),
            config.agents.max_concurrent
        ));
    }

    // Resolve agent name (auto-generate if not provided)
    let name_was_auto = opts.name.map(|n| n.trim().is_empty()).unwrap_or(true);
    let mut agent_name = if name_was_auto {
        format!("{capability}-{task_id}")
    } else {
        opts.name.unwrap().trim().to_string()
    };

    if name_was_auto {
        let taken: Vec<String> = active_sessions
            .iter()
            .map(|s| s.agent_name.clone())
            .collect();
        agent_name = generate_agent_name(capability, task_id, &taken);
    } else {
        // Check uniqueness for explicit names
        if let Some(existing) = session_store
            .get_by_name(&agent_name)
            .map_err(|e| e.to_string())?
        {
            if existing.state != AgentState::Zombie && existing.state != AgentState::Completed {
                return Err(format!(
                    "Agent name \"{}\" is already in use (state: {})",
                    agent_name, existing.state
                ));
            }
        }
    }

    // Task lock check (except when parent is delegating its own task)
    let lock_holder = active_sessions
        .iter()
        .find(|s| s.task_id == task_id)
        .map(|s| s.agent_name.clone());
    if let Some(ref holder) = lock_holder {
        if Some(holder.as_str()) != opts.parent {
            return Err(format!(
                "Task \"{task_id}\" is already being worked by agent \"{holder}\"."
            ));
        }
    }

    // Stagger delay
    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
    let stagger_ms =
        calculate_stagger_delay(config.agents.stagger_delay_ms, &active_sessions, now_ms);
    if stagger_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(stagger_ms));
    }

    // Per-lead agent limit
    if let Some(parent) = opts.parent {
        let max_per_lead = opts
            .max_agents
            .unwrap_or(config.agents.max_agents_per_lead);
        if max_per_lead > 0 {
            let count = active_sessions
                .iter()
                .filter(|s| s.parent_agent.as_deref() == Some(parent))
                .count();
            if count as u32 >= max_per_lead {
                return Err(format!(
                    "Per-lead agent limit reached: \"{parent}\" has {count}/{max_per_lead} active children."
                ));
            }
        }
    }

    // Validate task exists (non-fatal if tracker unavailable)
    if config.task_tracker.enabled && !opts.skip_task_check {
        match Command::new("sd").args(["show", task_id]).output() {
            Ok(out) if !out.status.success() => {
                let err = String::from_utf8_lossy(&out.stderr);
                return Err(format!(
                    "Task \"{task_id}\" not found or inaccessible: {err}"
                ));
            }
            Err(_) => {
                // sd not available — non-fatal
            }
            _ => {}
        }
    }

    // Create worktree
    let worktree_base_dir = root.join(&config.worktrees.base_dir);
    fs::create_dir_all(&worktree_base_dir)
        .map_err(|e| format!("Failed to create worktree base dir: {e}"))?;

    let base_branch = opts
        .base_branch
        .map(|s| s.to_string())
        .or_else(|| get_current_branch(&root))
        .unwrap_or_else(|| config.project.canonical_branch.clone());

    let branch_name = format!("overstory/{agent_name}/{task_id}");
    let worktree_path = worktree_base_dir.join(&agent_name);

    // Check if worktree directory already exists on disk (catches completed sessions
    // where the DB record no longer shows as active but the directory remains).
    if worktree_path.exists() {
        return Err(format!(
            "Worktree directory already exists: {}. \
             Agent name \"{}\" may already be in use — choose a different name with --name.",
            worktree_path.display(),
            agent_name,
        ));
    }

    create_worktree(&root, &base_branch, &branch_name, &worktree_path)
        .map_err(|e| format!("Failed to create worktree: {e}"))?;

    // Everything from here needs rollback on failure
    let result = do_spawn(DoSpawnContext {
        config: &config,
        root: &root,
        overstory_dir: &overstory_dir,
        sessions_db_str: &sessions_db_str,
        session_store: &session_store,
        agent_name: &agent_name,
        capability,
        task_id,
        run_id: &run_id,
        depth: opts.depth,
        parent_agent: opts.parent,
        spec_path: spec_path.as_deref(),
        file_scope: &file_scope,
        agent_def,
        worktree_path: &worktree_path,
        branch_name: &branch_name,
        runtime_name: opts.runtime,
        no_directives: opts.no_directives,
        skip_review: opts.skip_review,
        max_agents_override: opts.dispatch_max_agents,
        headless: opts.headless,
        json: opts.json,
    });

    if result.is_err() {
        rollback_worktree(&root, &worktree_path, &branch_name);
    }
    result
}

// ---------------------------------------------------------------------------
// Internal spawn context
// ---------------------------------------------------------------------------

struct DoSpawnContext<'a> {
    config: &'a crate::types::OverstoryConfig,
    root: &'a Path,
    overstory_dir: &'a Path,
    sessions_db_str: &'a str,
    session_store: &'a SessionStore,
    agent_name: &'a str,
    capability: &'a str,
    task_id: &'a str,
    run_id: &'a str,
    depth: u32,
    parent_agent: Option<&'a str>,
    spec_path: Option<&'a Path>,
    file_scope: &'a [String],
    agent_def: &'a crate::types::AgentDefinition,
    worktree_path: &'a Path,
    branch_name: &'a str,
    runtime_name: Option<&'a str>,
    no_directives: bool,
    skip_review: bool,
    max_agents_override: Option<u32>,
    /// If true, force headless spawn regardless of runtime.is_headless()
    headless: bool,
    json: bool,
}

fn do_spawn(ctx: DoSpawnContext<'_>) -> Result<(), String> {
    // Resolve runtime
    let runtime_id = ctx
        .runtime_name
        .or_else(|| {
            ctx.config
                .runtime
                .as_ref()
                .map(|r| r.default.as_str())
        })
        .unwrap_or("claude");
    let runtime = get_runtime(runtime_id)?;

    // Mulch domains
    let mulch_domains = if ctx.config.mulch.enabled {
        ctx.config.mulch.domains.clone()
    } else {
        vec![]
    };

    // Load base agent definition
    let agent_def_path = ctx
        .root
        .join(&ctx.config.agents.base_dir)
        .join(&ctx.agent_def.file);
    let base_definition = fs::read_to_string(&agent_def_path)
        .unwrap_or_else(|_| format!("# {}\n", ctx.capability));

    let spec_path_str = ctx
        .spec_path
        .map(|p| p.to_string_lossy().to_string());

    // Build overlay config
    let overlay_config = OverlayConfig {
        agent_name: ctx.agent_name.to_string(),
        task_id: ctx.task_id.to_string(),
        spec_path: spec_path_str.clone(),
        branch_name: ctx.branch_name.to_string(),
        worktree_path: ctx.worktree_path.to_string_lossy().to_string(),
        file_scope: ctx.file_scope.to_vec(),
        mulch_domains,
        parent_agent: ctx.parent_agent.map(|s| s.to_string()),
        depth: ctx.depth,
        can_spawn: ctx.agent_def.can_spawn,
        capability: ctx.capability.to_string(),
        base_definition,
        mulch_expertise: None,
        mulch_records: None,
        no_directives: Some(ctx.no_directives),
        skip_scout: None,
        skip_review: Some(ctx.skip_review),
        max_agents_override: ctx.max_agents_override,
        tracker_cli: Some("sd".to_string()),
        tracker_name: Some("seeds".to_string()),
        quality_gates: ctx.config.project.quality_gates.clone(),
        instruction_path: Some(runtime.instruction_path().to_string()),
        verification: ctx.config.project.verification.clone(),
    };

    // Write overlay
    write_overlay(
        ctx.worktree_path,
        &overlay_config,
        ctx.root,
        runtime.instruction_path(),
    )?;

    // Deploy hooks config (writes settings.local.json for Claude)
    let hooks_def = HooksDef {
        agent_name: ctx.agent_name.to_string(),
        capability: ctx.capability.to_string(),
        worktree_path: ctx.worktree_path.to_string_lossy().to_string(),
        quality_gates: ctx.config.project.quality_gates.clone(),
    };
    // Deploy config writes settings.local.json and hooks; overlay already written above.
    // Pass empty string for overlay_content since write_overlay handled it.
    runtime.deploy_config(ctx.worktree_path, "", &hooks_def)?;

    // Send auto-dispatch mail (pre-spawn so SessionStart hook can find it)
    let mail_db = ctx.overstory_dir.join("mail.db");
    let mail_db_str = mail_db.to_string_lossy().to_string();
    let from_agent = ctx.parent_agent.unwrap_or("orchestrator");
    let dispatch_body = build_dispatch_body(
        ctx.task_id,
        ctx.capability,
        spec_path_str.as_deref(),
        runtime.instruction_path(),
    );
    if let Ok(mail_store) = MailStore::new(&mail_db_str) {
        let _ = mail_store.insert(&InsertMailMessage {
            id: None,
            from_agent: from_agent.to_string(),
            to_agent: ctx.agent_name.to_string(),
            subject: format!("Dispatch: {}", ctx.task_id),
            body: dispatch_body,
            priority: MailPriority::Normal,
            message_type: MailMessageType::Dispatch,
            thread_id: None,
            payload: None,
        });
    }

    // Claim task in tracker (non-fatal)
    if ctx.config.task_tracker.enabled {
        let _ = Command::new("sd")
            .args(["update", ctx.task_id, "--status", "in_progress"])
            .output();
    }

    // Create agent identity if new
    let identity_dir = ctx.overstory_dir.join("agents").join(ctx.agent_name);
    if !identity_dir.exists() {
        fs::create_dir_all(&identity_dir)
            .map_err(|e| format!("Failed to create identity dir: {e}"))?;
        let identity = AgentIdentity {
            name: ctx.agent_name.to_string(),
            capability: ctx.capability.to_string(),
            created: chrono::Utc::now().to_rfc3339(),
            sessions_completed: 0,
            expertise_domains: ctx.config.mulch.domains.clone(),
            recent_tasks: vec![],
        };
        let identity_json = serde_json::to_string_pretty(&identity)
            .map_err(|e| format!("Failed to serialize identity: {e}"))?;
        fs::write(identity_dir.join("identity.json"), identity_json)
            .map_err(|e| format!("Failed to write identity: {e}"))?;
    }

    // Resolve model string
    let model = ctx
        .config
        .models
        .get(ctx.capability)
        .cloned()
        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
    let resolved_model = ResolvedModel {
        model: model.clone(),
        env: None,
        is_explicit_override: Some(false),
    };

    // Create log dir for this agent session
    let log_timestamp = chrono::Utc::now()
        .to_rfc3339()
        .replace([':', '.'], "-");
    let agent_log_dir = ctx
        .overstory_dir
        .join("logs")
        .join(ctx.agent_name)
        .join(&log_timestamp);
    fs::create_dir_all(&agent_log_dir)
        .map_err(|e| format!("Failed to create log dir: {e}"))?;

    // Spawn the agent — headless flag overrides runtime default
    let (pid, tmux_session) = if ctx.headless || runtime.is_headless() {
        spawn_headless(ctx.agent_name, ctx.task_id, ctx.worktree_path, &agent_log_dir, runtime.as_ref(), &resolved_model, ctx.parent_agent, ctx.depth)?
    } else {
        spawn_tmux(&ctx.config.project.name, ctx.agent_name, ctx.capability, ctx.task_id, ctx.worktree_path, runtime.as_ref(), &resolved_model, ctx.parent_agent, ctx.depth)?
    };

    // Record session in DB
    let session = AgentSession {
        id: format!(
            "session-{}-{}",
            chrono::Utc::now().timestamp_millis(),
            ctx.agent_name
        ),
        agent_name: ctx.agent_name.to_string(),
        capability: ctx.capability.to_string(),
        worktree_path: ctx.worktree_path.to_string_lossy().to_string(),
        branch_name: ctx.branch_name.to_string(),
        task_id: ctx.task_id.to_string(),
        tmux_session: tmux_session.clone(),
        state: AgentState::Booting,
        pid,
        parent_agent: ctx.parent_agent.map(|s| s.to_string()),
        depth: ctx.depth,
        run_id: Some(ctx.run_id.to_string()),
        started_at: chrono::Utc::now().to_rfc3339(),
        last_activity: chrono::Utc::now().to_rfc3339(),
        escalation_level: 0,
        stalled_since: None,
        transcript_path: None,
    };
    ctx.session_store
        .upsert(&session)
        .map_err(|e| e.to_string())?;

    // Increment run agent count
    let run_store = RunStore::new(ctx.sessions_db_str).map_err(|e| e.to_string())?;
    run_store
        .increment_agent_count(ctx.run_id)
        .map_err(|e| e.to_string())?;

    // Output
    if ctx.json {
        println!(
            "{}",
            json_output(
                "sling",
                &serde_json::json!({
                    "agentName": ctx.agent_name,
                    "capability": ctx.capability,
                    "taskId": ctx.task_id,
                    "branch": ctx.branch_name,
                    "worktree": ctx.worktree_path.to_string_lossy(),
                    "tmuxSession": tmux_session,
                    "pid": pid,
                })
            )
        );
    } else {
        let heading = if ctx.headless || runtime.is_headless() {
            "Agent launched (headless)"
        } else {
            "Agent launched"
        };
        print_success(heading, Some(ctx.agent_name));
        println!("   Task:     {}", ctx.task_id);
        println!("   Branch:   {}", ctx.branch_name);
        println!("   Worktree: {}", ctx.worktree_path.display());
        if !tmux_session.is_empty() {
            println!("   Tmux:     {tmux_session}");
        }
        if let Some(p) = pid {
            println!("   PID:      {p}");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Spawn helpers
// ---------------------------------------------------------------------------

/// Spawn a headless agent (stdin/stdout pipes), write beacon, orphan process.
#[allow(clippy::too_many_arguments)]
fn spawn_headless(
    agent_name: &str,
    task_id: &str,
    worktree_path: &Path,
    log_dir: &Path,
    runtime: &dyn crate::runtimes::AgentRuntime,
    model: &ResolvedModel,
    parent_agent: Option<&str>,
    depth: u32,
) -> Result<(Option<i64>, String), String> {
    let spawn_opts = SpawnOpts {
        model: model.model.clone(),
        cwd: worktree_path.to_string_lossy().to_string(),
        permission_mode: "bypassPermissions".to_string(),
        allowed_tools: vec![
            "Read".to_string(),
            "Write".to_string(),
            "Edit".to_string(),
            "Bash".to_string(),
            "Glob".to_string(),
            "Grep".to_string(),
            "Agent".to_string(),
        ],
        instruction_path: runtime.instruction_path().to_string(),
    };
    let argv = runtime.build_headless_command(&spawn_opts);

    let stdout_file = fs::File::create(log_dir.join("stdout.log"))
        .map_err(|e| format!("Failed to create stdout.log: {e}"))?;
    let stderr_file = fs::File::create(log_dir.join("stderr.log"))
        .map_err(|e| format!("Failed to create stderr.log: {e}"))?;

    let binary = argv.first().ok_or("Empty spawn command")?;
    let args = &argv[1..];
    let env_map = runtime.build_env(model);

    let mut cmd = Command::new(binary);
    cmd.args(args)
        .current_dir(worktree_path)
        .stdout(stdout_file)
        .stderr(stderr_file)
        .stdin(std::process::Stdio::piped())
        .env("OVERSTORY_AGENT_NAME", agent_name)
        .env(
            "OVERSTORY_WORKTREE_PATH",
            worktree_path.to_string_lossy().as_ref(),
        );
    for (k, v) in &env_map {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn agent: {e}"))?;
    let pid = child.id() as i64;

    // Write beacon to stdin then signal EOF
    let beacon = build_beacon(
        agent_name,
        &spawn_opts.model, // placeholder — not used in headless beacon text
        task_id,
        parent_agent,
        depth,
        runtime.instruction_path(),
    );
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(beacon.as_bytes());
        let _ = stdin.flush();
        // Drop stdin → EOF to child
    }

    // Orphan: drop child without waiting (child runs as orphan in background)
    std::mem::forget(child);

    Ok((Some(pid), String::new()))
}

/// Spawn an interactive tmux session and send the beacon.
#[allow(clippy::too_many_arguments)]
fn spawn_tmux(
    project_name: &str,
    agent_name: &str,
    capability: &str,
    task_id: &str,
    worktree_path: &Path,
    runtime: &dyn crate::runtimes::AgentRuntime,
    model: &ResolvedModel,
    parent_agent: Option<&str>,
    depth: u32,
) -> Result<(Option<i64>, String), String> {
    tmux::ensure_tmux_available()?;

    let tmux_name = format!("overstory-{project_name}-{agent_name}");
    let spawn_opts = SpawnOpts {
        model: model.model.clone(),
        cwd: worktree_path.to_string_lossy().to_string(),
        permission_mode: "bypassPermissions".to_string(),
        allowed_tools: vec![
            "Read".to_string(),
            "Write".to_string(),
            "Edit".to_string(),
            "Bash".to_string(),
            "Glob".to_string(),
            "Grep".to_string(),
            "Agent".to_string(),
        ],
        instruction_path: runtime.instruction_path().to_string(),
    };
    let command = runtime.build_interactive_command(&spawn_opts);
    let tmux_pid = tmux::create_session(&tmux_name, worktree_path, &command)?;

    // Brief wait for TUI initialization
    std::thread::sleep(std::time::Duration::from_millis(2_000));

    let beacon = build_beacon(
        agent_name,
        capability,
        task_id,
        parent_agent,
        depth,
        runtime.instruction_path(),
    );
    let _ = tmux::send_keys(&tmux_name, &beacon);

    Ok((Some(tmux_pid as i64), tmux_name))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_agent_name_base() {
        let taken: Vec<String> = vec![];
        let name = generate_agent_name("builder", "task-001", &taken);
        assert_eq!(name, "builder-task-001");
    }

    #[test]
    fn test_generate_agent_name_collision() {
        let taken = vec!["builder-task-001".to_string()];
        let name = generate_agent_name("builder", "task-001", &taken);
        assert_eq!(name, "builder-task-001-2");
    }

    #[test]
    fn test_generate_agent_name_multiple_collisions() {
        let taken = vec![
            "builder-task-001".to_string(),
            "builder-task-001-2".to_string(),
            "builder-task-001-3".to_string(),
        ];
        let name = generate_agent_name("builder", "task-001", &taken);
        assert_eq!(name, "builder-task-001-4");
    }

    #[test]
    fn test_calculate_stagger_delay_zero_delay() {
        let sessions = vec![];
        let delay = calculate_stagger_delay(0, &sessions, 100_000);
        assert_eq!(delay, 0);
    }

    #[test]
    fn test_calculate_stagger_delay_no_sessions() {
        let delay = calculate_stagger_delay(5_000, &[], 100_000);
        assert_eq!(delay, 0);
    }

    #[test]
    fn test_validate_hierarchy_lead_ok() {
        let result = validate_hierarchy(None, "lead", false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_hierarchy_builder_ok() {
        let result = validate_hierarchy(None, "builder", false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_hierarchy_merger_no_parent_rejected() {
        let result = validate_hierarchy(None, "merger", false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("merger"));
    }

    #[test]
    fn test_validate_hierarchy_force_bypasses() {
        let result = validate_hierarchy(None, "merger", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_dispatch_body_with_spec() {
        let body = build_dispatch_body("task-001", "builder", Some("/specs/task.md"), ".claude/CLAUDE.md");
        assert!(body.contains("task-001"));
        assert!(body.contains("builder"));
        assert!(body.contains("/specs/task.md"));
    }

    #[test]
    fn test_build_dispatch_body_no_spec() {
        let body = build_dispatch_body("task-001", "builder", None, ".claude/CLAUDE.md");
        assert!(body.contains("No spec file provided"));
    }

    #[test]
    fn test_build_beacon_format() {
        let beacon = build_beacon("agent-x", "builder", "task-001", Some("lead-a"), 2, ".claude/CLAUDE.md");
        assert!(beacon.contains("[OVERSTORY]"));
        assert!(beacon.contains("agent-x"));
        assert!(beacon.contains("builder"));
        assert!(beacon.contains("task-001"));
        assert!(beacon.contains("Depth: 2"));
        assert!(beacon.contains("Parent: lead-a"));
    }

    #[test]
    fn test_get_current_branch() {
        let branch = get_current_branch(std::path::Path::new("."));
        // Should either be Some(branch_name) or None — just verify no panic
        let _ = branch;
    }
}
