//! `grove update` — refresh .overstory/ managed files from embedded defaults.
//!
//! Compares existing project files with grove's built-in defaults (embedded at
//! compile time). Overwrites only files that differ. Supports `--dry-run` to
//! preview changes without writing.
//!
//! Managed files:
//! - `.overstory/agent-defs/*.md`    (from embedded agent definitions)
//! - `.overstory/agent-manifest.json`
//! - `.overstory/hooks.json`
//! - `.overstory/.gitignore`
//! - `.overstory/README.md`

use std::fs;
use std::path::Path;

use crate::commands::init::{
    build_agent_manifest, build_hooks_json, OVERSTORY_GITIGNORE, OVERSTORY_README,
};
use crate::json::json_output;
use crate::logging::{print_hint, print_success};

// ---------------------------------------------------------------------------
// Embedded agent definition files
// ---------------------------------------------------------------------------

/// Embedded agent definitions, excluding deprecated supervisor.md.
const AGENT_DEFS: &[(&str, &str)] = &[
    ("builder.md", include_str!("../../agents/builder.md")),
    (
        "coordinator.md",
        include_str!("../../agents/coordinator.md"),
    ),
    ("lead.md", include_str!("../../agents/lead.md")),
    ("merger.md", include_str!("../../agents/merger.md")),
    ("monitor.md", include_str!("../../agents/monitor.md")),
    (
        "orchestrator.md",
        include_str!("../../agents/orchestrator.md"),
    ),
    ("reviewer.md", include_str!("../../agents/reviewer.md")),
    ("scout.md", include_str!("../../agents/scout.md")),
    ("verifier.md", include_str!("../../agents/verifier.md")),
];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub struct UpdateOptions {
    pub agents: bool,
    pub manifest: bool,
    pub hooks: bool,
    pub dry_run: bool,
    pub json: bool,
}

struct UpdateResult {
    agent_defs_updated: Vec<String>,
    agent_defs_unchanged: Vec<String>,
    manifest_updated: bool,
    hooks_updated: bool,
    gitignore_updated: bool,
    readme_updated: bool,
}

/// Entry point for `grove update`.
pub fn execute(opts: UpdateOptions, project: Option<&Path>) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let project_root = project.unwrap_or(&cwd);
    let overstory_dir = project_root.join(".overstory");

    // Verify project is initialized
    if !overstory_dir.join("config.yaml").exists() {
        return Err("Not initialized. Run 'grove init' first to set up .overstory/".to_string());
    }

    // Determine what to refresh — no granular flags means refresh all
    let has_granular = opts.agents || opts.manifest || opts.hooks;
    let do_agents = if has_granular { opts.agents } else { true };
    let do_manifest = if has_granular { opts.manifest } else { true };
    let do_hooks = if has_granular { opts.hooks } else { true };
    let do_gitignore = !has_granular;
    let do_readme = !has_granular;

    let mut result = UpdateResult {
        agent_defs_updated: Vec::new(),
        agent_defs_unchanged: Vec::new(),
        manifest_updated: false,
        hooks_updated: false,
        gitignore_updated: false,
        readme_updated: false,
    };

    // 1. Agent definition files
    if do_agents {
        let defs_dir = overstory_dir.join("agent-defs");
        if !defs_dir.exists() && !opts.dry_run {
            fs::create_dir_all(&defs_dir).map_err(|e| e.to_string())?;
        }
        for (filename, content) in AGENT_DEFS {
            let target = defs_dir.join(filename);
            let needs_update = match fs::read_to_string(&target) {
                Ok(existing) => existing != *content,
                Err(_) => true,
            };
            if needs_update {
                if !opts.dry_run {
                    fs::write(&target, content).map_err(|e| e.to_string())?;
                }
                result.agent_defs_updated.push(filename.to_string());
            } else {
                result.agent_defs_unchanged.push(filename.to_string());
            }
        }
    }

    // 2. agent-manifest.json
    if do_manifest {
        let manifest_path = overstory_dir.join("agent-manifest.json");
        let new_content = format!(
            "{}\n",
            serde_json::to_string_pretty(&build_agent_manifest()).map_err(|e| e.to_string())?
        );
        let needs_update = match fs::read_to_string(&manifest_path) {
            Ok(existing) => existing != new_content,
            Err(_) => true,
        };
        if needs_update {
            if !opts.dry_run {
                fs::write(&manifest_path, &new_content).map_err(|e| e.to_string())?;
            }
            result.manifest_updated = true;
        }
    }

    // 3. hooks.json
    if do_hooks {
        let hooks_path = overstory_dir.join("hooks.json");
        let new_content = build_hooks_json();
        let needs_update = match fs::read_to_string(&hooks_path) {
            Ok(existing) => existing != new_content,
            Err(_) => true,
        };
        if needs_update {
            if !opts.dry_run {
                fs::write(&hooks_path, &new_content).map_err(|e| e.to_string())?;
            }
            result.hooks_updated = true;
        }
    }

    // 4. .gitignore
    if do_gitignore {
        let gitignore_path = overstory_dir.join(".gitignore");
        let needs_update = match fs::read_to_string(&gitignore_path) {
            Ok(existing) => existing != OVERSTORY_GITIGNORE,
            Err(_) => true,
        };
        if needs_update {
            if !opts.dry_run {
                fs::write(&gitignore_path, OVERSTORY_GITIGNORE).map_err(|e| e.to_string())?;
            }
            result.gitignore_updated = true;
        }
    }

    // 5. README.md
    if do_readme {
        let readme_path = overstory_dir.join("README.md");
        let needs_update = match fs::read_to_string(&readme_path) {
            Ok(existing) => existing != OVERSTORY_README,
            Err(_) => true,
        };
        if needs_update {
            if !opts.dry_run {
                fs::write(&readme_path, OVERSTORY_README).map_err(|e| e.to_string())?;
            }
            result.readme_updated = true;
        }
    }

    // Output
    if opts.json {
        let payload = serde_json::json!({
            "dry_run": opts.dry_run,
            "agent_defs": {
                "updated": result.agent_defs_updated,
                "unchanged": result.agent_defs_unchanged,
            },
            "manifest": { "updated": result.manifest_updated },
            "hooks": { "updated": result.hooks_updated },
            "gitignore": { "updated": result.gitignore_updated },
            "readme": { "updated": result.readme_updated },
        });
        json_output("update", &payload);
        return Ok(());
    }

    let prefix = if opts.dry_run {
        "Would update"
    } else {
        "Updated"
    };
    let mut any_changed = false;

    for f in &result.agent_defs_updated {
        print_success(prefix, Some(&format!("agent-defs/{f}")));
        any_changed = true;
    }
    if result.manifest_updated {
        print_success(prefix, Some("agent-manifest.json"));
        any_changed = true;
    }
    if result.hooks_updated {
        print_success(prefix, Some("hooks.json"));
        if !opts.dry_run {
            print_hint("If hooks are deployed, run 'grove hooks install --force' to redeploy");
        }
        any_changed = true;
    }
    if result.gitignore_updated {
        print_success(prefix, Some(".gitignore"));
        any_changed = true;
    }
    if result.readme_updated {
        print_success(prefix, Some("README.md"));
        any_changed = true;
    }

    if !any_changed {
        print_success("Already up to date", None);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_project(dir: &TempDir) {
        let ov = dir.path().join(".overstory");
        fs::create_dir_all(&ov).unwrap();
        fs::write(ov.join("config.yaml"), "name: test\n").unwrap();
    }

    #[test]
    fn test_update_dry_run_no_error() {
        let dir = TempDir::new().unwrap();
        setup_project(&dir);
        let result = execute(
            UpdateOptions {
                agents: false,
                manifest: true,
                hooks: false,
                dry_run: true,
                json: false,
            },
            Some(dir.path()),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_requires_init() {
        let dir = TempDir::new().unwrap();
        // No .overstory/config.yaml
        let result = execute(
            UpdateOptions {
                agents: false,
                manifest: true,
                hooks: false,
                dry_run: true,
                json: false,
            },
            Some(dir.path()),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_agent_defs_embedded() {
        assert!(!AGENT_DEFS.is_empty(), "should have embedded agent defs");
        for (name, content) in AGENT_DEFS {
            assert!(!name.is_empty());
            assert!(!content.is_empty(), "agent def {name} should not be empty");
        }
    }
}
