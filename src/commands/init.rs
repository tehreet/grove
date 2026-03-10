//! `grove init` — initialize .overstory/ directory structure.
//!
//! Scaffolds the `.overstory/` directory in the current project with:
//! - config.yaml (serialized from OverstoryConfig defaults)
//! - agent-manifest.json (starter agent definitions)
//! - hooks.json (central hooks config)
//! - Required subdirectories (agents/, agent-defs/, worktrees/, specs/, logs/)
//! - .gitignore for runtime state files
//! - README.md explaining the directory

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::json::json_output;
use crate::logging::{print_hint, print_success, print_warning};
use crate::types::{AgentDefinition, AgentManifest, OverstoryConfig};

const OVERSTORY_DIR: &str = ".overstory";

/// Overlay template embedded at compile time so `grove init` can write it
/// into new projects even without access to the grove source tree.
pub const OVERLAY_TEMPLATE: &str = include_str!("../../templates/overlay.md.tmpl");

// ---------------------------------------------------------------------------
// Git detection helpers
// ---------------------------------------------------------------------------

fn detect_project_name(root: &Path) -> String {
    if let Ok(output) = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(root)
        .output()
    {
        if output.status.success() {
            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Some(last) = url.split('/').next_back() {
                let name = last.trim_end_matches(".git");
                if !name.is_empty() {
                    return name.to_string();
                }
            }
        }
    }
    root.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn detect_canonical_branch(root: &Path) -> String {
    if let Ok(output) = Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .current_dir(root)
        .output()
    {
        if output.status.success() {
            let r = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Some(branch) = r.split('/').next_back() {
                if !branch.is_empty() {
                    return branch.to_string();
                }
            }
        }
    }
    if let Ok(output) = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(root)
        .output()
    {
        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !branch.is_empty() {
                return branch;
            }
        }
    }
    "main".to_string()
}

// ---------------------------------------------------------------------------
// Content builders (pub for testing)
// ---------------------------------------------------------------------------

/// Build the starter agent manifest.
pub fn build_agent_manifest() -> AgentManifest {
    #[allow(clippy::type_complexity)]
    let role_specs: &[(&str, &str, &str, &[&str], &[&str], bool, &[&str])] = &[
        (
            "scout",
            "scout.md",
            "haiku",
            &["Read", "Glob", "Grep", "Bash"],
            &["explore", "research"],
            false,
            &["read-only"],
        ),
        (
            "builder",
            "builder.md",
            "sonnet",
            &["Read", "Write", "Edit", "Glob", "Grep", "Bash"],
            &["implement", "refactor", "fix"],
            false,
            &[],
        ),
        (
            "reviewer",
            "reviewer.md",
            "sonnet",
            &["Read", "Glob", "Grep", "Bash"],
            &["review", "validate"],
            false,
            &["read-only"],
        ),
        (
            "lead",
            "lead.md",
            "opus",
            &["Read", "Write", "Edit", "Glob", "Grep", "Bash", "Task"],
            &["coordinate", "implement", "review"],
            true,
            &[],
        ),
        (
            "merger",
            "merger.md",
            "sonnet",
            &["Read", "Write", "Edit", "Glob", "Grep", "Bash"],
            &["merge", "resolve-conflicts"],
            false,
            &[],
        ),
        (
            "coordinator",
            "coordinator.md",
            "opus",
            &["Read", "Glob", "Grep", "Bash"],
            &["coordinate", "dispatch", "escalate"],
            true,
            &["read-only", "no-worktree"],
        ),
        (
            "monitor",
            "monitor.md",
            "sonnet",
            &["Read", "Glob", "Grep", "Bash"],
            &["monitor", "patrol"],
            false,
            &["read-only", "no-worktree"],
        ),
        (
            "verifier",
            "verifier.md",
            "sonnet",
            &["Read", "Glob", "Grep", "Bash"],
            &["verification", "browser-testing"],
            false,
            &[
                "READ_ONLY: Cannot modify files",
                "Must use --session flag for browser isolation",
                "Must clean up browser session on exit",
            ],
        ),
    ];

    let mut agents: HashMap<String, AgentDefinition> = HashMap::new();
    for &(name, file, model, tools, capabilities, can_spawn, constraints) in role_specs {
        agents.insert(
            name.to_string(),
            AgentDefinition {
                file: file.to_string(),
                model: model.to_string(),
                tools: tools.iter().map(|s| s.to_string()).collect(),
                capabilities: capabilities.iter().map(|s| s.to_string()).collect(),
                can_spawn,
                constraints: constraints.iter().map(|s| s.to_string()).collect(),
            },
        );
    }

    let mut capability_index: HashMap<String, Vec<String>> = HashMap::new();
    for (name, def) in &agents {
        for cap in &def.capabilities {
            capability_index
                .entry(cap.clone())
                .or_default()
                .push(name.clone());
        }
    }

    AgentManifest {
        version: "1.0".to_string(),
        agents,
        capability_index,
    }
}

/// Build the hooks.json content for the project orchestrator.
///
/// Returns pretty-printed JSON with tab indentation (matches Biome formatting).
pub fn build_hooks_json() -> String {
    let tool_name_extract =
        r#"read -r INPUT; TOOL_NAME=$(echo "$INPUT" | sed 's/.*"tool_name": *"\([^"]*\)".*/\1/');"#;

    let hooks = serde_json::json!({
        "hooks": {
            "SessionStart": [{
                "matcher": "",
                "hooks": [{"type": "command", "command": "ov prime --agent orchestrator"}]
            }],
            "UserPromptSubmit": [{
                "matcher": "",
                "hooks": [{"type": "command", "command": "ov mail check --inject --agent orchestrator"}]
            }],
            "PreToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [{
                        "type": "command",
                        "command": r#"read -r INPUT; CMD=$(echo "$INPUT" | sed 's/.*"command": *"\([^"]*\)".*/\1/'); if echo "$CMD" | grep -qE '\bgit\s+push\b'; then echo '{"decision":"block","reason":"git push is blocked by overstory \u2014 merge locally, push manually when ready"}'; exit 0; fi;"#
                    }]
                },
                {
                    "matcher": "",
                    "hooks": [{
                        "type": "command",
                        "command": format!("{tool_name_extract} ov log tool-start --agent orchestrator --tool-name \"$TOOL_NAME\"")
                    }]
                }
            ],
            "PostToolUse": [
                {
                    "matcher": "",
                    "hooks": [{
                        "type": "command",
                        "command": format!("{tool_name_extract} ov log tool-end --agent orchestrator --tool-name \"$TOOL_NAME\"")
                    }]
                },
                {
                    "matcher": "Bash",
                    "hooks": [{
                        "type": "command",
                        "command": r#"read -r INPUT; if echo "$INPUT" | grep -q 'git commit'; then mulch diff HEAD~1 2>/dev/null || true; fi"#
                    }]
                }
            ],
            "Stop": [{
                "matcher": "",
                "hooks": [
                    {"type": "command", "command": "ov log session-end --agent orchestrator"},
                    {"type": "command", "command": "mulch learn"}
                ]
            }],
            "PreCompact": [{
                "matcher": "",
                "hooks": [{"type": "command", "command": "ov prime --agent orchestrator --compact"}]
            }]
        }
    });

    let pretty = serde_json::to_string_pretty(&hooks).unwrap_or_default();
    format!("{}\n", to_tab_indented(&pretty))
}

/// Convert serde_json pretty-printed JSON (2-space indent) to tab-indented.
fn to_tab_indented(s: &str) -> String {
    s.lines()
        .map(|line| {
            let leading = line.len() - line.trim_start().len();
            let tabs = leading / 2;
            format!("{}{}", "\t".repeat(tabs), line.trim_start())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Content for .overstory/.gitignore
pub const OVERSTORY_GITIGNORE: &str = "\
# Wildcard+whitelist: ignore everything, whitelist tracked files
# Auto-healed by ov prime on each session start
*
!.gitignore
!config.yaml
!agent-manifest.json
!hooks.json
!groups.json
!agent-defs/
!agent-defs/**
!README.md
";

/// Content for .overstory/README.md
pub const OVERSTORY_README: &str = "\
# .overstory/

This directory is managed by [overstory](https://github.com/jayminwest/overstory) — a multi-agent orchestration system for Claude Code.

Overstory turns a single Claude Code session into a multi-agent team by spawning worker agents in git worktrees as direct child processes, coordinating them through a custom SQLite mail system, and merging their work back with tiered conflict resolution.

## Key Commands

- `ov init`          — Initialize this directory
- `ov status`        — Show active agents and state
- `ov sling <id>`    — Spawn a worker agent
- `ov mail check`    — Check agent messages
- `ov merge`         — Merge agent work back
- `ov dashboard`     — Live TUI monitoring
- `ov doctor`        — Run health checks

## Structure

- `config.yaml`             — Project configuration
- `agent-manifest.json`     — Agent registry
- `hooks.json`              — Claude Code hooks config
- `agent-defs/`             — Agent definition files (.md)
- `specs/`                  — Task specifications
- `agents/`                 — Per-agent state and identity
- `worktrees/`              — Git worktrees (gitignored)
- `logs/`                   — Agent logs (gitignored)
";

// ---------------------------------------------------------------------------
// Ecosystem tools
// ---------------------------------------------------------------------------

struct SiblingTool {
    name: &'static str,
    cli: &'static str,
    dot_dir: &'static str,
}

const SIBLING_TOOLS: &[SiblingTool] = &[
    SiblingTool {
        name: "mulch",
        cli: "ml",
        dot_dir: ".mulch",
    },
    SiblingTool {
        name: "seeds",
        cli: "sd",
        dot_dir: ".seeds",
    },
    SiblingTool {
        name: "canopy",
        cli: "cn",
        dot_dir: ".canopy",
    },
];

fn is_tool_installed(cli: &str) -> bool {
    Command::new(cli)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn init_sibling_tool(tool: &SiblingTool, project_root: &Path) -> &'static str {
    if !is_tool_installed(tool.cli) {
        print_warning(
            &format!("{} not installed — skipping", tool.name),
            Some(&format!("install: npm i -g @os-eco/{}-cli", tool.name)),
        );
        return "skipped";
    }
    let result = Command::new(tool.cli)
        .arg("init")
        .current_dir(project_root)
        .output();
    match result {
        Ok(o) if o.status.success() => {
            print_success(&format!("Bootstrapped {}", tool.name), None);
            "initialized"
        }
        _ => {
            // Check if already initialized
            if project_root.join(tool.dot_dir).exists() {
                "already_initialized"
            } else {
                print_warning(&format!("{} init failed", tool.name), None);
                "skipped"
            }
        }
    }
}

fn onboard_sibling_tool(tool: &SiblingTool, project_root: &Path) {
    if !is_tool_installed(tool.cli) {
        return;
    }
    let _ = Command::new(tool.cli)
        .arg("onboard")
        .current_dir(project_root)
        .status();
}

fn bootstrap_ecosystem_tools<'a>(
    tool_set: &'a [&'a SiblingTool],
    project_root: &Path,
) -> Vec<(&'a str, &'static str)> {
    if !tool_set.is_empty() {
        println!("\nBootstrapping ecosystem tools...\n");
    }

    let mut tool_statuses = Vec::with_capacity(tool_set.len());
    for tool in tool_set {
        let status = init_sibling_tool(tool, project_root);
        tool_statuses.push((tool.name, status));
    }

    tool_statuses
}

fn setup_gitattributes(project_root: &Path) -> bool {
    let entries = [
        ".mulch/expertise/*.jsonl merge=union",
        ".seeds/issues.jsonl merge=union",
    ];
    let gitattrs_path = project_root.join(".gitattributes");
    let existing = fs::read_to_string(&gitattrs_path).unwrap_or_default();
    let missing: Vec<&str> = entries
        .iter()
        .filter(|e| !existing.contains(*e))
        .copied()
        .collect();
    if missing.is_empty() {
        return false;
    }
    let separator = if !existing.is_empty() && !existing.ends_with('\n') {
        "\n"
    } else {
        ""
    };
    let new_content = format!("{}{}{}\n", existing, separator, missing.join("\n"));
    fs::write(&gitattrs_path, new_content).is_ok()
}

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InitOutput {
    project: String,
    path: String,
    scaffold_committed: bool,
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

/// Options for `grove init`.
pub struct InitOptions<'a> {
    pub name: Option<String>,
    pub yes: bool,
    pub force: bool,
    pub tools: Option<String>,
    pub skip_mulch: bool,
    pub skip_seeds: bool,
    pub skip_canopy: bool,
    pub skip_onboard: bool,
    pub json: bool,
    pub project_override: Option<&'a Path>,
}

pub fn execute(opts: InitOptions<'_>) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let project_root: PathBuf = opts.project_override.map(PathBuf::from).unwrap_or(cwd);

    // 0. Verify git repo
    let git_check = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(&project_root)
        .output()
        .map_err(|e| e.to_string())?;
    if !git_check.status.success() {
        return Err("overstory requires a git repository. Run 'git init' first.".to_string());
    }

    let ov_path = project_root.join(OVERSTORY_DIR);
    let config_yaml_path = ov_path.join("config.yaml");

    // 1. Check if already initialized
    if config_yaml_path.exists() {
        if !opts.force && !opts.yes {
            println!(
                "Warning: .overstory/ already initialized in this project.\n\
                 Use --force or --yes to reinitialize."
            );
            return Ok(());
        }
        let flag = if opts.yes { "--yes" } else { "--force" };
        println!("Reinitializing .overstory/ ({flag})\n");
    }

    // 2. Detect project info
    let project_name = opts
        .name
        .clone()
        .unwrap_or_else(|| detect_project_name(&project_root));
    let canonical_branch = detect_canonical_branch(&project_root);

    println!("Initializing overstory for \"{project_name}\"...\n");

    // 3. Create directory structure
    let dirs = [
        ov_path.clone(),
        ov_path.join("agents"),
        ov_path.join("agent-defs"),
        ov_path.join("worktrees"),
        ov_path.join("specs"),
        ov_path.join("logs"),
    ];
    for dir in &dirs {
        fs::create_dir_all(dir).map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
        let rel = dir.strip_prefix(&project_root).unwrap_or(dir);
        print_success("Created", Some(&format!("{}/", rel.display())));
    }

    // 4. Write config.yaml
    let mut config = OverstoryConfig::default();
    config.project.name = project_name.clone();
    config.project.root = project_root.to_string_lossy().into_owned();
    config.project.canonical_branch = canonical_branch;

    let config_yaml =
        serde_yaml::to_string(&config).map_err(|e| format!("Failed to serialize config: {e}"))?;
    let config_with_header = format!(
        "# Overstory configuration\n# See: https://github.com/overstory/overstory\n\n{config_yaml}"
    );
    fs::write(&config_yaml_path, &config_with_header)
        .map_err(|e| format!("Failed to write config.yaml: {e}"))?;
    print_success("Created", Some(&format!("{OVERSTORY_DIR}/config.yaml")));

    // 5. Write agent-manifest.json
    let manifest = build_agent_manifest();
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Failed to serialize manifest: {e}"))?;
    let manifest_path = ov_path.join("agent-manifest.json");
    fs::write(
        &manifest_path,
        format!("{}\n", to_tab_indented(&manifest_json)),
    )
    .map_err(|e| format!("Failed to write agent-manifest.json: {e}"))?;
    print_success(
        "Created",
        Some(&format!("{OVERSTORY_DIR}/agent-manifest.json")),
    );

    // 6. Write hooks.json
    let hooks_content = build_hooks_json();
    let hooks_path = ov_path.join("hooks.json");
    fs::write(&hooks_path, &hooks_content)
        .map_err(|e| format!("Failed to write hooks.json: {e}"))?;
    print_success("Created", Some(&format!("{OVERSTORY_DIR}/hooks.json")));

    // 7. Write .gitignore
    let gitignore_path = ov_path.join(".gitignore");
    fs::write(&gitignore_path, OVERSTORY_GITIGNORE)
        .map_err(|e| format!("Failed to write .gitignore: {e}"))?;
    print_success("Created", Some(&format!("{OVERSTORY_DIR}/.gitignore")));

    // 7b. Write README.md
    let readme_path = ov_path.join("README.md");
    fs::write(&readme_path, OVERSTORY_README)
        .map_err(|e| format!("Failed to write README.md: {e}"))?;
    print_success("Created", Some(&format!("{OVERSTORY_DIR}/README.md")));

    // 7c. Write templates/overlay.md.tmpl so `grove sling` works in this project
    let templates_dir = project_root.join("templates");
    fs::create_dir_all(&templates_dir).map_err(|e| format!("Failed to create templates/: {e}"))?;
    let template_path = templates_dir.join("overlay.md.tmpl");
    if !template_path.exists() {
        fs::write(&template_path, OVERLAY_TEMPLATE)
            .map_err(|e| format!("Failed to write templates/overlay.md.tmpl: {e}"))?;
        print_success("Created", Some("templates/overlay.md.tmpl"));
    }

    // 8. Bootstrap sibling ecosystem tools
    let tool_set: Vec<&SiblingTool> = SIBLING_TOOLS
        .iter()
        .filter(|t| {
            if let Some(ref requested) = opts.tools {
                let names: Vec<&str> = requested.split(',').map(str::trim).collect();
                return names.contains(&t.name);
            }
            !(t.name == "mulch" && opts.skip_mulch
                || t.name == "seeds" && opts.skip_seeds
                || t.name == "canopy" && opts.skip_canopy)
        })
        .collect();

    let tool_statuses = bootstrap_ecosystem_tools(&tool_set, &project_root);

    // 9. Set up .gitattributes
    let gitattrs_updated = setup_gitattributes(&project_root);
    if gitattrs_updated {
        print_success("Created", Some(".gitattributes"));
    }

    // 10. Run onboard for each tool
    if !opts.skip_onboard {
        for (tool, status) in &tool_statuses {
            if *status != "skipped" {
                if let Some(t) = SIBLING_TOOLS.iter().find(|t| t.name == *tool) {
                    onboard_sibling_tool(t, &project_root);
                }
            }
        }
    }

    // 11. Auto-commit scaffold files
    let mut paths_to_add = vec![OVERSTORY_DIR.to_string()];
    if project_root.join(".gitattributes").exists() {
        paths_to_add.push(".gitattributes".to_string());
    }
    if project_root.join("CLAUDE.md").exists() {
        paths_to_add.push("CLAUDE.md".to_string());
    }
    for tool in SIBLING_TOOLS {
        if project_root.join(tool.dot_dir).exists() {
            paths_to_add.push(tool.dot_dir.to_string());
        }
    }

    let mut scaffold_committed = false;
    let add_result = Command::new("git")
        .arg("add")
        .args(&paths_to_add)
        .current_dir(&project_root)
        .status();

    if add_result.map(|s| s.success()).unwrap_or(false) {
        let diff_result = Command::new("git")
            .args(["diff", "--cached", "--quiet"])
            .current_dir(&project_root)
            .status();
        let has_staged = diff_result.map(|s| !s.success()).unwrap_or(false);
        if has_staged {
            let commit_result = Command::new("git")
                .args([
                    "commit",
                    "-m",
                    "chore: initialize overstory and ecosystem tools",
                ])
                .current_dir(&project_root)
                .status();
            if commit_result.map(|s| s.success()).unwrap_or(false) {
                print_success("Committed", Some("scaffold files"));
                scaffold_committed = true;
            } else {
                print_warning("Scaffold commit failed", None);
            }
        }
    } else {
        print_warning("Scaffold commit skipped", Some("git add failed"));
    }

    // 12. Output
    if opts.json {
        let output = InitOutput {
            project: project_name,
            path: ov_path.to_string_lossy().into_owned(),
            scaffold_committed,
        };
        println!("{}", json_output("init", &output));
    } else {
        print_success("Initialized", None);
        print_hint("Next: run `grove hooks install` to enable Claude Code hooks.");
        print_hint("Then: run `grove status` to see the current state.");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_build_agent_manifest_has_all_roles() {
        let manifest = build_agent_manifest();
        let expected_roles = [
            "scout",
            "builder",
            "reviewer",
            "lead",
            "merger",
            "coordinator",
            "monitor",
            "verifier",
        ];
        for role in &expected_roles {
            assert!(
                manifest.agents.contains_key(*role),
                "manifest missing role: {role}"
            );
        }
        assert_eq!(manifest.version, "1.0");
    }

    #[test]
    fn test_build_agent_manifest_capability_index() {
        let manifest = build_agent_manifest();
        // lead has "coordinate" capability
        let coordinators = manifest.capability_index.get("coordinate");
        assert!(coordinators.is_some());
        let coordinators = coordinators.unwrap();
        assert!(coordinators.contains(&"lead".to_string()));
        assert!(coordinators.contains(&"coordinator".to_string()));
    }

    #[test]
    fn test_build_agent_manifest_lead_can_spawn() {
        let manifest = build_agent_manifest();
        assert!(manifest.agents["lead"].can_spawn);
        assert!(!manifest.agents["builder"].can_spawn);
        assert!(!manifest.agents["scout"].can_spawn);
    }

    #[test]
    fn test_build_hooks_json_is_valid_json() {
        let hooks = build_hooks_json();
        let parsed: serde_json::Value = serde_json::from_str(&hooks).expect("valid JSON");
        assert!(parsed.get("hooks").is_some());
        let hooks_obj = &parsed["hooks"];
        assert!(hooks_obj.get("SessionStart").is_some());
        assert!(hooks_obj.get("PreToolUse").is_some());
        assert!(hooks_obj.get("PostToolUse").is_some());
        assert!(hooks_obj.get("Stop").is_some());
    }

    #[test]
    fn test_to_tab_indented() {
        let input = "{\n  \"key\": {\n    \"nested\": true\n  }\n}";
        let result = to_tab_indented(input);
        assert!(result.contains("\t\"key\""));
        assert!(result.contains("\t\t\"nested\""));
    }

    #[test]
    fn test_overstory_gitignore_contains_whitelist() {
        assert!(OVERSTORY_GITIGNORE.contains("!config.yaml"));
        assert!(OVERSTORY_GITIGNORE.contains("!agent-manifest.json"));
        assert!(OVERSTORY_GITIGNORE.contains("!hooks.json"));
        assert!(OVERSTORY_GITIGNORE.contains("!agent-defs/"));
    }

    #[test]
    fn test_detect_project_name_falls_back_to_dirname() {
        // Use /tmp which always has no git remote
        let name = detect_project_name(Path::new("/tmp/my-project-name"));
        // If /tmp has no git remote, falls back to dirname
        // The important thing is it returns a non-empty string
        assert!(!name.is_empty());
    }

    #[test]
    fn test_setup_gitattributes_adds_missing_entries() {
        let dir = TempDir::new().unwrap();
        let added = setup_gitattributes(dir.path());
        assert!(added, "should add entries to new .gitattributes");
        let content = fs::read_to_string(dir.path().join(".gitattributes")).unwrap();
        assert!(content.contains(".mulch/expertise/*.jsonl merge=union"));
        assert!(content.contains(".seeds/issues.jsonl merge=union"));
    }

    #[test]
    fn test_setup_gitattributes_idempotent() {
        let dir = TempDir::new().unwrap();
        setup_gitattributes(dir.path());
        let added_again = setup_gitattributes(dir.path());
        assert!(!added_again, "should not re-add existing entries");
    }

    #[test]
    fn test_build_agent_manifest_serializes_to_json() {
        let manifest = build_agent_manifest();
        let json = serde_json::to_string(&manifest).expect("serializes");
        assert!(json.contains("capabilityIndex"));
        assert!(json.contains("builder"));
    }
}
