#![allow(dead_code)]

use crate::config::resolve_project_root;
use crate::db::merge_queue::MergeQueue;
use crate::logging;
use crate::merge::resolver::{run_git, MergeResolver, MergeResolverOptions};
use crate::types::{MergeEntry, MergeEntryStatus, ResolutionTier};

pub fn execute(
    branch: Option<String>,
    all: bool,
    into: Option<String>,
    dry_run: bool,
    json: bool,
    project: Option<&std::path::Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let project_root = resolve_project_root(&cwd, project).map_err(|e| e.to_string())?;
    let db_path = project_root
        .join(".overstory")
        .join("merge-queue.db")
        .to_string_lossy()
        .to_string();
    let repo_root = project_root.to_string_lossy().to_string();

    let canonical_branch = into.as_deref().unwrap_or("main").to_string();

    let resolver = MergeResolver::new(MergeResolverOptions {
        ai_resolve_enabled: false,
        reimagine_enabled: false,
    });

    if let Some(branch_name) = branch {
        // --branch mode
        let queue = MergeQueue::new(&db_path).map_err(|e| e.to_string())?;
        let all_entries = queue.list(None).map_err(|e| e.to_string())?;
        let entry = all_entries
            .into_iter()
            .find(|e| e.branch_name == branch_name)
            .unwrap_or_else(|| MergeEntry {
                id: 0,
                branch_name: branch_name.clone(),
                task_id: String::new(),
                agent_name: String::new(),
                files_modified: vec![],
                enqueued_at: chrono::Utc::now().to_rfc3339(),
                status: MergeEntryStatus::Pending,
                resolved_tier: None,
            });

        if dry_run {
            let out = run_git(
                &repo_root,
                &[
                    "merge",
                    "--no-commit",
                    "--no-ff",
                    entry.branch_name.as_str(),
                ],
            )?;
            // Abort immediately after the dry-run attempt
            run_git(&repo_root, &["merge", "--abort"]).ok();
            if out.exit_code == 0 {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "branch": branch_name,
                            "dry_run": true,
                            "has_conflicts": false,
                            "conflict_files": [],
                        })
                    );
                } else {
                    println!(
                        "{} {}",
                        logging::color_green("✓"),
                        logging::color_green(&format!("No conflicts for {branch_name}"))
                    );
                }
            } else {
                let cf_out = run_git(&repo_root, &["diff", "--name-only", "--diff-filter=U"])?;
                let conflict_files: Vec<String> = cf_out
                    .stdout
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| l.to_string())
                    .collect();
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "branch": branch_name,
                            "dry_run": true,
                            "has_conflicts": true,
                            "conflict_files": conflict_files,
                        })
                    );
                } else {
                    println!(
                        "{} {} conflict(s) in: {}",
                        logging::color_yellow("⚠"),
                        conflict_files.len(),
                        conflict_files.join(", ")
                    );
                }
            }
            return Ok(());
        }

        let outcome = resolver.resolve(&entry, &canonical_branch, &repo_root)?;
        if outcome.result.success {
            queue
                .update_status(
                    &branch_name,
                    MergeEntryStatus::Merged,
                    outcome.result.entry.resolved_tier,
                )
                .map_err(|e| e.to_string())?;
        } else {
            queue
                .update_status(&branch_name, MergeEntryStatus::Conflict, None)
                .map_err(|e| e.to_string())?;
        }
        print_outcome(&outcome, json);
        return Ok(());
    }

    if all {
        // --all mode
        let mut queue = MergeQueue::new(&db_path).map_err(|e| e.to_string())?;
        let mut results = Vec::new();
        while let Some(entry) = queue.dequeue().map_err(|e| e.to_string())? {
            let branch_name = entry.branch_name.clone();
            let outcome = resolver.resolve(&entry, &canonical_branch, &repo_root)?;
            if outcome.result.success {
                queue
                    .update_status(
                        &branch_name,
                        MergeEntryStatus::Merged,
                        outcome.result.entry.resolved_tier,
                    )
                    .map_err(|e| e.to_string())?;
            } else {
                queue
                    .update_status(&branch_name, MergeEntryStatus::Conflict, None)
                    .map_err(|e| e.to_string())?;
            }
            results.push(outcome);
        }
        if json {
            let json_results: Vec<_> = results
                .iter()
                .map(|o| serde_json::to_value(o).unwrap_or_default())
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&json_results).unwrap_or_default()
            );
        } else {
            if results.is_empty() {
                println!("{}", logging::muted("No pending merge entries."));
            }
            for outcome in &results {
                print_outcome(outcome, false);
            }
        }
        return Ok(());
    }

    // No --branch, no --all
    println!("Usage: grove merge --branch <branch> | --all [--into <branch>] [--dry-run] [--json]");
    Ok(())
}

fn print_outcome(outcome: &crate::types::MergeOutcome, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(outcome).unwrap_or_default()
        );
        return;
    }

    let branch = &outcome.result.entry.branch_name;
    if outcome.result.success {
        let tier_label = match outcome.result.tier {
            ResolutionTier::CleanMerge => "clean merge",
            ResolutionTier::AutoResolve => "auto-resolve",
            ResolutionTier::AiResolve => "ai-resolve",
            ResolutionTier::Reimagine => "reimagine",
        };
        println!(
            "{} {}",
            logging::color_green("✓"),
            logging::color_green(&format!("Merged {branch} via {tier_label}"))
        );
        let displaced: Vec<_> = outcome
            .resolutions
            .iter()
            .filter(|r| !r.displaced_hunks.is_empty())
            .collect();
        if !displaced.is_empty() {
            println!(
                "{} Content displaced in {} file(s):",
                logging::color_yellow("⚠"),
                displaced.len()
            );
            for res in &displaced {
                println!("  - {}", res.file);
            }
        }
    } else {
        let err = outcome
            .result
            .error_message
            .as_deref()
            .unwrap_or("unknown error");
        println!(
            "{} Failed to merge {branch}: {err}",
            logging::color_red("✗")
        );
    }
}
