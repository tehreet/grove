//! Merge resolver — tiers 1 (clean merge) and 2 (auto-resolve).
//!
//! Port of `reference/merge-resolver.ts` (tiers 1-2 only).
//! Tier 3 (AI-resolve) and tier 4 (reimagine) are not yet implemented.

use std::fs;
use std::process::Command;

use crate::types::{
    ConflictResolution, DisplacedHunk, MergeEntry, MergeEntryStatus, MergeOutcome, MergeResult,
    ResolutionTier,
};

// ---------------------------------------------------------------------------
// OS-Eco state file detection
// ---------------------------------------------------------------------------

const OS_ECO_STATE_PREFIXES: &[&str] = &[
    ".seeds/",
    ".overstory/",
    ".greenhouse/",
    ".mulch/",
    ".canopy/",
    ".claude/",
];
const OS_ECO_STATE_FILES: &[&str] = &["CLAUDE.md"];

pub(crate) fn is_os_eco_state_file(path: &str) -> bool {
    for prefix in OS_ECO_STATE_PREFIXES {
        if path.starts_with(prefix) {
            return true;
        }
    }
    for file in OS_ECO_STATE_FILES {
        if path == *file {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Git helper
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub(crate) struct GitOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub(crate) fn run_git(repo_root: &str, args: &[&str]) -> Result<GitOutput, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))?;
    Ok(GitOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

// ---------------------------------------------------------------------------
// Dirty working tree handling
// ---------------------------------------------------------------------------

fn check_dirty_working_tree(repo_root: &str) -> Result<Vec<String>, String> {
    let out = run_git(repo_root, &["status", "--porcelain"])?;
    let files: Vec<String> = out
        .stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l[3..].trim().to_string())
        .collect();
    Ok(files)
}

fn auto_commit_state_files(repo_root: &str, state_files: &[String]) -> Result<bool, String> {
    if state_files.is_empty() {
        return Ok(false);
    }
    for file in state_files {
        run_git(repo_root, &["add", file.as_str()])?;
    }
    let out = run_git(
        repo_root,
        &[
            "commit",
            "-m",
            "auto-commit: os-eco state files before merge",
        ],
    )?;
    Ok(out.exit_code == 0)
}

// ---------------------------------------------------------------------------
// Conflict marker parsing — line-by-line state machine
// ---------------------------------------------------------------------------

enum ConflictState {
    Normal,
    InCanonical,
    InIncoming,
}

/// Parse conflict markers and keep incoming (agent) changes.
/// Returns (resolved_content, displaced_hunks) or None if no markers found.
pub fn resolve_conflicts_keep_incoming(
    content: &str,
    file: &str,
) -> Option<(String, Vec<DisplacedHunk>)> {
    if !content.contains("<<<<<<< ") {
        return None;
    }

    let mut state = ConflictState::Normal;
    let mut output = String::new();
    let mut canonical_lines: Vec<String> = Vec::new();
    let mut incoming_lines: Vec<String> = Vec::new();
    let mut hunks: Vec<DisplacedHunk> = Vec::new();
    let mut conflict_start_line: usize = 0;
    let mut line_num: usize = 0;

    for line in content.lines() {
        line_num += 1;
        match state {
            ConflictState::Normal => {
                if line.starts_with("<<<<<<< ") {
                    state = ConflictState::InCanonical;
                    conflict_start_line = line_num;
                    canonical_lines.clear();
                    incoming_lines.clear();
                } else {
                    output.push_str(line);
                    output.push('\n');
                }
            }
            ConflictState::InCanonical => {
                if line == "=======" {
                    state = ConflictState::InIncoming;
                } else {
                    canonical_lines.push(line.to_string());
                }
            }
            ConflictState::InIncoming => {
                if line.starts_with(">>>>>>> ") {
                    // Emit incoming lines to output
                    for inc in &incoming_lines {
                        output.push_str(inc);
                        output.push('\n');
                    }
                    // Record displacement if canonical had meaningful content
                    let canonical_content = canonical_lines.join("\n")
                        + if canonical_lines.is_empty() { "" } else { "\n" };
                    let incoming_content = incoming_lines.join("\n")
                        + if incoming_lines.is_empty() { "" } else { "\n" };
                    if !canonical_content.trim().is_empty() {
                        hunks.push(DisplacedHunk {
                            file: file.to_string(),
                            canonical_content,
                            incoming_content,
                            line_start: conflict_start_line,
                        });
                    }
                    canonical_lines.clear();
                    incoming_lines.clear();
                    state = ConflictState::Normal;
                } else {
                    incoming_lines.push(line.to_string());
                }
            }
        }
    }

    Some((output, hunks))
}

/// Parse conflict markers and keep ALL lines from both sides (union strategy).
/// Returns resolved content or None if no markers found.
pub fn resolve_conflicts_union(content: &str) -> Option<String> {
    if !content.contains("<<<<<<< ") {
        return None;
    }

    let mut state = ConflictState::Normal;
    let mut output = String::new();
    let mut canonical_lines: Vec<String> = Vec::new();
    let mut incoming_lines: Vec<String> = Vec::new();

    for line in content.lines() {
        match state {
            ConflictState::Normal => {
                if line.starts_with("<<<<<<< ") {
                    state = ConflictState::InCanonical;
                    canonical_lines.clear();
                    incoming_lines.clear();
                } else {
                    output.push_str(line);
                    output.push('\n');
                }
            }
            ConflictState::InCanonical => {
                if line == "=======" {
                    state = ConflictState::InIncoming;
                } else {
                    canonical_lines.push(line.to_string());
                }
            }
            ConflictState::InIncoming => {
                if line.starts_with(">>>>>>> ") {
                    for canon in &canonical_lines {
                        output.push_str(canon);
                        output.push('\n');
                    }
                    for inc in &incoming_lines {
                        output.push_str(inc);
                        output.push('\n');
                    }
                    canonical_lines.clear();
                    incoming_lines.clear();
                    state = ConflictState::Normal;
                } else {
                    incoming_lines.push(line.to_string());
                }
            }
        }
    }

    Some(output)
}

// ---------------------------------------------------------------------------
// Merge union check
// ---------------------------------------------------------------------------

fn check_merge_union(repo_root: &str, file_path: &str) -> bool {
    let out = match run_git(repo_root, &["check-attr", "merge", "--", file_path]) {
        Ok(o) => o,
        Err(_) => return false,
    };
    out.stdout.trim_end().ends_with(": merge: union")
}

// ---------------------------------------------------------------------------
// Tier 1: Clean Merge
// ---------------------------------------------------------------------------

struct CleanMergeResult {
    success: bool,
    conflict_files: Vec<String>,
}

fn try_clean_merge(entry: &MergeEntry, repo_root: &str) -> Result<CleanMergeResult, String> {
    let out = run_git(
        repo_root,
        &["merge", "--no-edit", entry.branch_name.as_str()],
    )?;
    if out.exit_code == 0 {
        return Ok(CleanMergeResult {
            success: true,
            conflict_files: vec![],
        });
    }
    // Gather conflict files
    let cf_out = run_git(repo_root, &["diff", "--name-only", "--diff-filter=U"])?;
    let conflict_files: Vec<String> = cf_out
        .stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect();
    Ok(CleanMergeResult {
        success: false,
        conflict_files,
    })
}

// ---------------------------------------------------------------------------
// Tier 2: Auto-Resolve
// ---------------------------------------------------------------------------

struct AutoResolveResult {
    success: bool,
    remaining_conflicts: Vec<String>,
    resolutions: Vec<ConflictResolution>,
}

fn try_auto_resolve(
    conflict_files: &[String],
    repo_root: &str,
) -> Result<AutoResolveResult, String> {
    let mut resolutions: Vec<ConflictResolution> = Vec::new();
    let mut remaining: Vec<String> = Vec::new();

    for file in conflict_files {
        let full_path = format!("{}/{}", repo_root, file);
        let content = match fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(e) => {
                remaining.push(file.clone());
                eprintln!("Warning: could not read {file}: {e}");
                continue;
            }
        };

        let use_union = check_merge_union(repo_root, file);

        if use_union {
            match resolve_conflicts_union(&content) {
                Some(resolved) => {
                    fs::write(&full_path, &resolved)
                        .map_err(|e| format!("Failed to write {file}: {e}"))?;
                    run_git(repo_root, &["add", file.as_str()])?;
                    resolutions.push(ConflictResolution {
                        file: file.clone(),
                        strategy: "union".to_string(),
                        displaced_hunks: vec![],
                    });
                }
                None => {
                    remaining.push(file.clone());
                }
            }
        } else {
            match resolve_conflicts_keep_incoming(&content, file) {
                Some((resolved, hunks)) => {
                    fs::write(&full_path, &resolved)
                        .map_err(|e| format!("Failed to write {file}: {e}"))?;
                    run_git(repo_root, &["add", file.as_str()])?;
                    resolutions.push(ConflictResolution {
                        file: file.clone(),
                        strategy: "keep-incoming".to_string(),
                        displaced_hunks: hunks,
                    });
                }
                None => {
                    remaining.push(file.clone());
                }
            }
        }
    }

    if remaining.is_empty() {
        run_git(repo_root, &["commit", "--no-edit"])?;
        Ok(AutoResolveResult {
            success: true,
            remaining_conflicts: vec![],
            resolutions,
        })
    } else {
        Ok(AutoResolveResult {
            success: false,
            remaining_conflicts: remaining,
            resolutions,
        })
    }
}

// ---------------------------------------------------------------------------
// Public resolver API
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub struct MergeResolverOptions {
    pub ai_resolve_enabled: bool,
    pub reimagine_enabled: bool,
}

pub struct MergeResolver {
    options: MergeResolverOptions,
}

impl MergeResolver {
    pub fn new(options: MergeResolverOptions) -> Self {
        Self { options }
    }

    pub fn resolve(
        &self,
        entry: &MergeEntry,
        canonical_branch: &str,
        repo_root: &str,
    ) -> Result<MergeOutcome, String> {
        let _ = &self.options; // options reserved for future tiers

        // 1. Ensure we're on the canonical branch
        let head_out = run_git(repo_root, &["symbolic-ref", "--short", "HEAD"])?;
        let current_branch = head_out.stdout.trim().to_string();
        if current_branch != canonical_branch {
            run_git(repo_root, &["checkout", canonical_branch])?;
        }

        // 2. Check dirty working tree
        let dirty_files = check_dirty_working_tree(repo_root)?;
        let state_files: Vec<String> = dirty_files
            .iter()
            .filter(|f| is_os_eco_state_file(f))
            .cloned()
            .collect();
        let non_state_dirty: Vec<String> = dirty_files
            .iter()
            .filter(|f| !is_os_eco_state_file(f))
            .cloned()
            .collect();

        // 3. Auto-commit state files
        let mut auto_committed: Vec<String> = Vec::new();
        if !state_files.is_empty() {
            auto_commit_state_files(repo_root, &state_files)?;
            auto_committed = state_files;
        }

        // 4. Stash remaining dirty files
        let mut stashed = false;
        if !non_state_dirty.is_empty() {
            let out = run_git(repo_root, &["stash", "push", "--include-untracked"])?;
            if out.exit_code == 0 {
                stashed = true;
            }
        }

        // 5. Tier 1: clean merge
        let tier1 = try_clean_merge(entry, repo_root)?;
        if tier1.success {
            if stashed {
                run_git(repo_root, &["stash", "pop"])?;
            }
            let result = MergeResult {
                entry: MergeEntry {
                    status: MergeEntryStatus::Merged,
                    resolved_tier: Some(ResolutionTier::CleanMerge),
                    ..entry.clone()
                },
                success: true,
                tier: ResolutionTier::CleanMerge,
                conflict_files: vec![],
                error_message: None,
            };
            return Ok(MergeOutcome {
                result,
                resolutions: vec![],
                auto_committed_state_files: auto_committed,
                stashed,
            });
        }

        // 6. Tier 2: auto-resolve
        let tier2 = try_auto_resolve(&tier1.conflict_files, repo_root)?;
        if tier2.success {
            if stashed {
                run_git(repo_root, &["stash", "pop"])?;
            }
            let result = MergeResult {
                entry: MergeEntry {
                    status: MergeEntryStatus::Merged,
                    resolved_tier: Some(ResolutionTier::AutoResolve),
                    ..entry.clone()
                },
                success: true,
                tier: ResolutionTier::AutoResolve,
                conflict_files: vec![],
                error_message: None,
            };
            return Ok(MergeOutcome {
                result,
                resolutions: tier2.resolutions,
                auto_committed_state_files: auto_committed,
                stashed,
            });
        }

        // 7. All tiers failed — abort
        run_git(repo_root, &["merge", "--abort"])?;
        if stashed {
            run_git(repo_root, &["stash", "pop"])?;
        }

        let remaining = tier2.remaining_conflicts;
        let result = MergeResult {
            entry: MergeEntry {
                status: MergeEntryStatus::Conflict,
                resolved_tier: None,
                ..entry.clone()
            },
            success: false,
            tier: ResolutionTier::AutoResolve,
            conflict_files: remaining.clone(),
            error_message: Some(format!(
                "Unresolved conflicts in {} file(s): {}",
                remaining.len(),
                remaining.join(", ")
            )),
        };
        Ok(MergeOutcome {
            result,
            resolutions: tier2.resolutions,
            auto_committed_state_files: auto_committed,
            stashed,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_conflicts_keep_incoming_basic() {
        let content =
            "before\n<<<<<<< HEAD\ncanonical line\n=======\nincoming line\n>>>>>>> branch\nafter\n";
        let result = resolve_conflicts_keep_incoming(content, "test.rs");
        assert!(result.is_some());
        let (resolved, hunks) = result.unwrap();
        assert_eq!(resolved, "before\nincoming line\nafter\n");
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].canonical_content, "canonical line\n");
        assert_eq!(hunks[0].incoming_content, "incoming line\n");
    }

    #[test]
    fn test_resolve_conflicts_keep_incoming_multiple() {
        let content = "a\n<<<<<<< HEAD\nb\n=======\nc\n>>>>>>> branch\nd\n<<<<<<< HEAD\ne\n=======\nf\n>>>>>>> branch\ng\n";
        let result = resolve_conflicts_keep_incoming(content, "test.rs");
        assert!(result.is_some());
        let (resolved, hunks) = result.unwrap();
        assert_eq!(resolved, "a\nc\nd\nf\ng\n");
        assert_eq!(hunks.len(), 2);
    }

    #[test]
    fn test_resolve_conflicts_keep_incoming_no_markers() {
        let content = "clean file\nno conflicts\n";
        assert!(resolve_conflicts_keep_incoming(content, "test.rs").is_none());
    }

    #[test]
    fn test_resolve_conflicts_union_basic() {
        let content = "before\n<<<<<<< HEAD\ncanonical\n=======\nincoming\n>>>>>>> branch\nafter\n";
        let result = resolve_conflicts_union(content);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "before\ncanonical\nincoming\nafter\n");
    }

    #[test]
    fn test_resolve_conflicts_union_no_markers() {
        assert!(resolve_conflicts_union("clean content\n").is_none());
    }

    #[test]
    fn test_is_os_eco_state_file() {
        assert!(is_os_eco_state_file(".seeds/issues/foo.md"));
        assert!(is_os_eco_state_file(".overstory/config.yaml"));
        assert!(is_os_eco_state_file(".claude/settings.json"));
        assert!(is_os_eco_state_file("CLAUDE.md"));
        assert!(!is_os_eco_state_file("src/main.rs"));
        assert!(!is_os_eco_state_file("Cargo.toml"));
    }

    #[test]
    fn test_displacement_tracking_empty_canonical() {
        // When canonical side is empty, no displacement recorded
        let content = "<<<<<<< HEAD\n=======\nnew content\n>>>>>>> branch\n";
        let result = resolve_conflicts_keep_incoming(content, "test.rs");
        assert!(result.is_some());
        let (resolved, hunks) = result.unwrap();
        assert_eq!(resolved, "new content\n");
        assert!(hunks.is_empty());
    }
}
