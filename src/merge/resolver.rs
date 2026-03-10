//! Merge resolver — tiers 1 (clean merge) and 2-4 (auto/AI resolve).
//!
//! Port of `reference/merge-resolver.ts`.

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

fn run_print_command(
    print_cmd_builder: &dyn Fn(&str) -> Vec<String>,
    prompt: &str,
) -> Result<String, String> {
    let argv = print_cmd_builder(prompt);
    if argv.is_empty() {
        return Err("print command builder returned empty argv".to_string());
    }

    let output = Command::new(&argv[0])
        .args(&argv[1..])
        .output()
        .map_err(|e| format!("failed to run print command: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "print command exited with status {}",
            output.status.code().unwrap_or(-1)
        ));
    }

    let resolved = String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string();
    if resolved.is_empty() {
        return Err("print command returned empty output".to_string());
    }

    Ok(resolved)
}

fn write_resolved_file(repo_root: &str, file: &str, resolved: &str) -> Result<(), String> {
    if resolved.contains("<<<<<<< ") {
        return Err("resolved content still contains conflict markers".to_string());
    }

    let full_path = format!("{repo_root}/{file}");
    fs::write(&full_path, resolved).map_err(|e| format!("Failed to write {file}: {e}"))?;
    run_git(repo_root, &["add", file])?;
    Ok(())
}

fn try_ai_resolve(
    conflict_files: &[String],
    repo_root: &str,
    print_cmd_builder: &dyn Fn(&str) -> Vec<String>,
) -> Result<AutoResolveResult, String> {
    let mut remaining = Vec::new();
    let mut resolutions = Vec::new();

    for file in conflict_files {
        let full_path = format!("{repo_root}/{file}");
        let content = match fs::read_to_string(&full_path) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("Warning: could not read {file}: {e}");
                remaining.push(file.clone());
                continue;
            }
        };

        let prompt = format!(
            "You are a merge resolver. The following file has git conflict markers. Resolve the conflicts by keeping the best of both sides. Return ONLY the resolved file content, no explanation, no markdown fences, just the raw file content.\n\nFile: {file}\n\n{content}"
        );

        let resolved = match run_print_command(print_cmd_builder, &prompt) {
            Ok(resolved) => resolved,
            Err(err) => {
                eprintln!("Warning: AI resolve failed for {file}: {err}");
                remaining.push(file.clone());
                continue;
            }
        };

        if let Err(err) = write_resolved_file(repo_root, file, &resolved) {
            eprintln!("Warning: AI resolve failed for {file}: {err}");
            remaining.push(file.clone());
            continue;
        }

        resolutions.push(ConflictResolution {
            file: file.clone(),
            strategy: "ai-resolve".to_string(),
            displaced_hunks: vec![],
        });
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

fn try_reimagine(
    conflict_files: &[String],
    repo_root: &str,
    print_cmd_builder: &dyn Fn(&str) -> Vec<String>,
    branch_name: &str,
) -> Result<AutoResolveResult, String> {
    let mut remaining = Vec::new();
    let mut resolutions = Vec::new();

    for file in conflict_files {
        let canonical = match run_git(repo_root, &["show", &format!("HEAD:{file}")]) {
            Ok(output) if output.exit_code == 0 => output.stdout,
            Ok(output) => {
                eprintln!(
                    "Warning: could not read canonical version for {file}: {}",
                    output.stderr.trim()
                );
                remaining.push(file.clone());
                continue;
            }
            Err(err) => {
                eprintln!("Warning: could not read canonical version for {file}: {err}");
                remaining.push(file.clone());
                continue;
            }
        };
        let incoming = match run_git(repo_root, &["show", &format!("{branch_name}:{file}")]) {
            Ok(output) if output.exit_code == 0 => output.stdout,
            Ok(output) => {
                eprintln!(
                    "Warning: could not read incoming version for {file}: {}",
                    output.stderr.trim()
                );
                remaining.push(file.clone());
                continue;
            }
            Err(err) => {
                eprintln!("Warning: could not read incoming version for {file}: {err}");
                remaining.push(file.clone());
                continue;
            }
        };

        let prompt = format!(
            "You are merging two versions of a file. Rewrite it to incorporate all changes from both versions. Return ONLY the file content.\n\nCANONICAL VERSION:\n{canonical}\n\nINCOMING VERSION:\n{incoming}"
        );

        let resolved = match run_print_command(print_cmd_builder, &prompt) {
            Ok(resolved) => resolved,
            Err(err) => {
                eprintln!("Warning: reimagine failed for {file}: {err}");
                remaining.push(file.clone());
                continue;
            }
        };

        if let Err(err) = write_resolved_file(repo_root, file, &resolved) {
            eprintln!("Warning: reimagine failed for {file}: {err}");
            remaining.push(file.clone());
            continue;
        }

        resolutions.push(ConflictResolution {
            file: file.clone(),
            strategy: "reimagine".to_string(),
            displaced_hunks: vec![],
        });
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
    print_runtime: Option<Box<dyn crate::runtimes::AgentRuntime>>,
}

impl MergeResolver {
    pub fn new(options: MergeResolverOptions) -> Self {
        Self {
            options,
            print_runtime: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_runtime(mut self, rt: Box<dyn crate::runtimes::AgentRuntime>) -> Self {
        self.print_runtime = Some(rt);
        self
    }

    pub fn resolve(
        &self,
        entry: &MergeEntry,
        canonical_branch: &str,
        repo_root: &str,
    ) -> Result<MergeOutcome, String> {
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

        // 7. Tier 3: AI resolve (if enabled)
        if self.options.ai_resolve_enabled {
            if let Some(ref rt) = self.print_runtime {
                let build_print = |prompt: &str| rt.build_print_command(prompt, None);
                let tier3 = try_ai_resolve(&tier2.remaining_conflicts, repo_root, &build_print)?;
                if tier3.success {
                    if stashed {
                        run_git(repo_root, &["stash", "pop"])?;
                    }
                    let result = MergeResult {
                        entry: MergeEntry {
                            status: MergeEntryStatus::Merged,
                            resolved_tier: Some(ResolutionTier::AiResolve),
                            ..entry.clone()
                        },
                        success: true,
                        tier: ResolutionTier::AiResolve,
                        conflict_files: vec![],
                        error_message: None,
                    };
                    let mut resolutions = tier2.resolutions;
                    resolutions.extend(tier3.resolutions);
                    return Ok(MergeOutcome {
                        result,
                        resolutions,
                        auto_committed_state_files: auto_committed,
                        stashed,
                    });
                }

                if self.options.reimagine_enabled {
                    let tier4 = try_reimagine(
                        &tier3.remaining_conflicts,
                        repo_root,
                        &build_print,
                        &entry.branch_name,
                    )?;
                    if tier4.success {
                        if stashed {
                            run_git(repo_root, &["stash", "pop"])?;
                        }
                        let result = MergeResult {
                            entry: MergeEntry {
                                status: MergeEntryStatus::Merged,
                                resolved_tier: Some(ResolutionTier::Reimagine),
                                ..entry.clone()
                            },
                            success: true,
                            tier: ResolutionTier::Reimagine,
                            conflict_files: vec![],
                            error_message: None,
                        };
                        let mut resolutions = tier2.resolutions;
                        resolutions.extend(tier3.resolutions);
                        resolutions.extend(tier4.resolutions);
                        return Ok(MergeOutcome {
                            result,
                            resolutions,
                            auto_committed_state_files: auto_committed,
                            stashed,
                        });
                    }

                    let remaining = tier4.remaining_conflicts;
                    let mut resolutions = tier2.resolutions;
                    resolutions.extend(tier3.resolutions);
                    resolutions.extend(tier4.resolutions);
                    run_git(repo_root, &["merge", "--abort"])?;
                    if stashed {
                        run_git(repo_root, &["stash", "pop"])?;
                    }

                    let result = MergeResult {
                        entry: MergeEntry {
                            status: MergeEntryStatus::Conflict,
                            resolved_tier: None,
                            ..entry.clone()
                        },
                        success: false,
                        tier: ResolutionTier::Reimagine,
                        conflict_files: remaining.clone(),
                        error_message: Some(format!(
                            "Unresolved conflicts in {} file(s): {}",
                            remaining.len(),
                            remaining.join(", ")
                        )),
                    };
                    return Ok(MergeOutcome {
                        result,
                        resolutions,
                        auto_committed_state_files: auto_committed,
                        stashed,
                    });
                }

                let remaining = tier3.remaining_conflicts;
                let mut resolutions = tier2.resolutions;
                resolutions.extend(tier3.resolutions);
                run_git(repo_root, &["merge", "--abort"])?;
                if stashed {
                    run_git(repo_root, &["stash", "pop"])?;
                }

                let result = MergeResult {
                    entry: MergeEntry {
                        status: MergeEntryStatus::Conflict,
                        resolved_tier: None,
                        ..entry.clone()
                    },
                    success: false,
                    tier: ResolutionTier::AiResolve,
                    conflict_files: remaining.clone(),
                    error_message: Some(format!(
                        "Unresolved conflicts in {} file(s): {}",
                        remaining.len(),
                        remaining.join(", ")
                    )),
                };
                return Ok(MergeOutcome {
                    result,
                    resolutions,
                    auto_committed_state_files: auto_committed,
                    stashed,
                });
            }
        }

        // 8. All tiers failed — abort
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

    #[test]
    fn test_try_ai_resolve_graceful_on_empty_output() {
        let repo = tempfile::tempdir().unwrap();
        let path = repo.path().join("conflict.txt");
        fs::write(
            &path,
            "<<<<<<< HEAD\ncanonical\n=======\nincoming\n>>>>>>> branch\n",
        )
        .unwrap();

        let result = try_ai_resolve(
            &[String::from("conflict.txt")],
            repo.path().to_str().unwrap(),
            &|_| vec!["sh".to_string(), "-c".to_string(), "printf ''".to_string()],
        )
        .unwrap();

        assert!(!result.success);
        assert_eq!(result.remaining_conflicts, vec!["conflict.txt"]);
    }

    #[test]
    fn test_try_reimagine_graceful_on_command_failure() {
        let repo = tempfile::tempdir().unwrap();
        let result = try_reimagine(
            &[String::from("conflict.txt")],
            repo.path().to_str().unwrap(),
            &|_| vec!["this-command-does-not-exist".to_string()],
            "feature",
        )
        .unwrap();

        assert!(!result.success);
        assert_eq!(result.remaining_conflicts, vec!["conflict.txt"]);
    }
}
