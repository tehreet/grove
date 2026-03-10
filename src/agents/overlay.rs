#![allow(dead_code)]

use std::fs;
use std::path::Path;

use crate::types::OverlayConfig;

/// Render the overlay template by replacing `{{VARIABLE}}` placeholders.
pub fn render_overlay(template: &str, config: &OverlayConfig) -> String {
    let mut out = template.to_string();

    // Basic fields
    out = out.replace("{{AGENT_NAME}}", &config.agent_name);
    out = out.replace("{{TASK_ID}}", &config.task_id);
    out = out.replace("{{BRANCH_NAME}}", &config.branch_name);
    out = out.replace("{{WORKTREE_PATH}}", &config.worktree_path);
    out = out.replace("{{DEPTH}}", &config.depth.to_string());
    out = out.replace("{{BASE_DEFINITION}}", &config.base_definition);

    // Optional spec path
    let spec_path = config
        .spec_path
        .as_deref()
        .unwrap_or("No spec file provided");
    out = out.replace("{{SPEC_PATH}}", spec_path);

    // Parent agent
    let parent_agent = config
        .parent_agent
        .as_deref()
        .unwrap_or("coordinator");
    out = out.replace("{{PARENT_AGENT}}", parent_agent);

    // File scope
    let file_scope = if config.file_scope.is_empty() {
        "No file scope restrictions".to_string()
    } else {
        config
            .file_scope
            .iter()
            .map(|f| format!("- `{f}`"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    out = out.replace("{{FILE_SCOPE}}", &file_scope);

    // Spec instruction
    let spec_instruction = if config.spec_path.is_some() {
        "Read your task spec at the path above. It contains the full description of\nwhat you need to build or review.".to_string()
    } else {
        String::new()
    };
    out = out.replace("{{SPEC_INSTRUCTION}}", &spec_instruction);

    // Mulch domains
    let mulch_domains = if config.mulch_domains.is_empty() {
        "No specific expertise domains configured".to_string()
    } else {
        let domain_list = config
            .mulch_domains
            .iter()
            .map(|d| format!("`{d}`"))
            .collect::<Vec<_>>()
            .join(" ");
        format!("```bash\nml prime {domain_list}\n```")
    };
    out = out.replace("{{MULCH_DOMAINS}}", &mulch_domains);

    // Mulch expertise (pre-loaded text)
    let mulch_expertise = config.mulch_expertise.as_deref().unwrap_or("");
    out = out.replace("{{MULCH_EXPERTISE}}", mulch_expertise);

    // Quality gates
    let quality_gates = render_quality_gates(config);
    out = out.replace("{{QUALITY_GATES}}", &quality_gates);

    // Spawn instructions
    let can_spawn = if config.can_spawn {
        "You may spawn sub-workers using `grove sling`. Ensure each worker has a clear, bounded task."
            .to_string()
    } else {
        "You may NOT spawn sub-workers.".to_string()
    };
    out = out.replace("{{CAN_SPAWN}}", &can_spawn);

    // Skip scout override
    let skip_scout = if config.skip_scout.unwrap_or(false) {
        "**Note:** Skip the scout phase — proceed directly to implementation.".to_string()
    } else {
        String::new()
    };
    out = out.replace("{{SKIP_SCOUT}}", &skip_scout);

    // Dispatch overrides
    let dispatch_overrides = render_dispatch_overrides(config);
    out = out.replace("{{DISPATCH_OVERRIDES}}", &dispatch_overrides);

    // Verification config
    let verification_config = render_verification(config);
    out = out.replace("{{VERIFICATION_CONFIG}}", &verification_config);

    // Constraints
    let constraints = render_constraints(config);
    out = out.replace("{{CONSTRAINTS}}", &constraints);

    out
}

fn render_quality_gates(config: &OverlayConfig) -> String {
    let tracker = config
        .tracker_cli
        .as_deref()
        .unwrap_or("sd");
    let _tracker_name = config
        .tracker_name
        .as_deref()
        .unwrap_or("seeds");

    let mut steps: Vec<String> = Vec::new();

    if let Some(ref gates) = config.quality_gates {
        for (i, gate) in gates.iter().enumerate() {
            steps.push(format!(
                "{}. **{}:** `{}` — {}",
                i + 1,
                gate.name,
                gate.command,
                gate.description
            ));
        }
        let next = steps.len() + 1;
        steps.push(format!(
            "{next}. **Commit:** all changes committed to your branch ({})",
            config.branch_name
        ));
        let next = steps.len() + 1;
        steps.push(format!(
            "{next}. **Record mulch learnings:** `ml record <domain> --type <convention|pattern|failure|decision> --description \"...\" --outcome-status success --outcome-agent {}` — capture insights from your work",
            config.agent_name
        ));
        let next = steps.len() + 1;
        steps.push(format!(
            "{next}. **Signal completion:** send `worker_done` mail to {}: `ov mail send --to {} --subject \"Worker done: {}\" --body \"Quality gates passed.\" --type worker_done --agent {}`",
            config.parent_agent.as_deref().unwrap_or("coordinator"),
            config.parent_agent.as_deref().unwrap_or("coordinator"),
            config.task_id,
            config.agent_name
        ));
        let next = steps.len() + 1;
        steps.push(format!(
            "{next}. **Close issue:** `{tracker} close {} --reason \"summary of changes\"`",
            config.task_id
        ));
    } else {
        steps.push(format!(
            "1. **Commit:** all changes committed to your branch ({})",
            config.branch_name
        ));
        steps.push(format!(
            "2. **Signal completion:** send `worker_done` mail to {}",
            config.parent_agent.as_deref().unwrap_or("coordinator")
        ));
        steps.push(format!(
            "3. **Close issue:** `{tracker} close {} --reason \"summary\"`",
            config.task_id
        ));
    }

    format!("## Quality Gates\n\nBefore reporting completion, you MUST pass all quality gates:\n\n{}", steps.join("\n"))
}

fn render_dispatch_overrides(config: &OverlayConfig) -> String {
    let mut parts = Vec::new();
    if config.skip_review.unwrap_or(false) {
        parts.push("**Note:** Skip the review phase.".to_string());
    }
    if let Some(max) = config.max_agents_override {
        parts.push(format!("**Max agents per lead:** {max}"));
    }
    parts.join("\n\n")
}

fn render_verification(config: &OverlayConfig) -> String {
    match &config.verification {
        None => String::new(),
        Some(v) => {
            let mut lines = vec!["## Verification".to_string(), String::new()];
            if let Some(cmd) = &v.dev_server_command {
                lines.push(format!("- **Dev server:** `{cmd}`"));
            }
            if let Some(url) = &v.base_url {
                lines.push(format!("- **Base URL:** {url}"));
            }
            if let Some(routes) = &v.routes {
                lines.push(format!("- **Routes:** {}", routes.join(", ")));
            }
            lines.join("\n")
        }
    }
}

fn render_constraints(config: &OverlayConfig) -> String {
    format!(
        "## Constraints\n\n\
        - **WORKTREE ISOLATION**: All writes MUST target files within your worktree at `{path}`\n\
        - NEVER write to the canonical repo root — all writes go to your worktree copy\n\
        - Only modify files in your File Scope\n\
        - Commit only to your branch: {branch}\n\
        - Never push to the canonical branch\n\
        - Report completion via `sd close` AND `ov mail send --type result`\n\
        - If you encounter a blocking issue, send mail with `--priority urgent --type error`",
        path = config.worktree_path,
        branch = config.branch_name,
    )
}

/// Overlay template embedded at compile time as a fallback when the on-disk
/// template is missing (e.g. in projects that haven't run `grove init` yet or
/// in older grove-initialized projects).
const EMBEDDED_OVERLAY_TEMPLATE: &str = include_str!("../../templates/overlay.md.tmpl");

/// Load the overlay template from disk and render it.
/// Falls back to the embedded template if the on-disk file doesn't exist.
pub fn render_overlay_from_template(project_root: &Path, config: &OverlayConfig) -> Result<String, String> {
    let template_path = project_root.join("templates/overlay.md.tmpl");
    let template = if template_path.exists() {
        fs::read_to_string(&template_path)
            .map_err(|e| format!("Failed to read overlay template at {}: {e}", template_path.display()))?
    } else {
        EMBEDDED_OVERLAY_TEMPLATE.to_string()
    };
    Ok(render_overlay(&template, config))
}

/// Render and write the overlay file to the given worktree at `instruction_path`.
pub fn write_overlay(
    worktree_path: &Path,
    config: &OverlayConfig,
    project_root: &Path,
    instruction_path: &str,
) -> Result<(), String> {
    let rendered = render_overlay_from_template(project_root, config)?;
    let dest = worktree_path.join(instruction_path);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create dir {}: {e}", parent.display()))?;
    }
    fs::write(&dest, rendered)
        .map_err(|e| format!("Failed to write overlay to {}: {e}", dest.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OverlayConfig, QualityGate, VerificationConfig};

    fn base_config() -> OverlayConfig {
        OverlayConfig {
            agent_name: "test-agent".to_string(),
            task_id: "task-001".to_string(),
            spec_path: Some("/specs/task-001.md".to_string()),
            branch_name: "overstory/test-agent/task-001".to_string(),
            worktree_path: "/worktrees/test-agent".to_string(),
            parent_agent: Some("lead-agent".to_string()),
            depth: 2,
            base_definition: "# Builder Agent\nYou are a builder.".to_string(),
            file_scope: vec!["src/foo.rs".to_string(), "src/bar.rs".to_string()],
            mulch_domains: vec!["rust".to_string()],
            can_spawn: false,
            capability: "builder".to_string(),
            mulch_expertise: None,
            mulch_records: None,
            no_directives: None,
            skip_scout: None,
            skip_review: None,
            max_agents_override: None,
            tracker_cli: None,
            tracker_name: None,
            quality_gates: None,
            instruction_path: None,
            verification: None,
        }
    }

    const SIMPLE_TEMPLATE: &str = "\
{{BASE_DEFINITION}}
Agent: {{AGENT_NAME}}
Task: {{TASK_ID}}
Spec: {{SPEC_PATH}}
Branch: {{BRANCH_NAME}}
Worktree: {{WORKTREE_PATH}}
Parent: {{PARENT_AGENT}}
Depth: {{DEPTH}}
Scope: {{FILE_SCOPE}}
Domains: {{MULCH_DOMAINS}}
{{MULCH_EXPERTISE}}
{{QUALITY_GATES}}
{{CAN_SPAWN}}
{{SKIP_SCOUT}}
{{DISPATCH_OVERRIDES}}
{{VERIFICATION_CONFIG}}
{{CONSTRAINTS}}
{{SPEC_INSTRUCTION}}";

    #[test]
    fn test_render_overlay_replaces_variables() {
        let config = base_config();
        let rendered = render_overlay(SIMPLE_TEMPLATE, &config);

        assert!(rendered.contains("test-agent"));
        assert!(rendered.contains("task-001"));
        assert!(rendered.contains("/specs/task-001.md"));
        assert!(rendered.contains("overstory/test-agent/task-001"));
        assert!(rendered.contains("/worktrees/test-agent"));
        assert!(rendered.contains("lead-agent"));
        assert!(rendered.contains("Depth: 2"));
        // No leftover placeholders
        assert!(!rendered.contains("{{"));
        assert!(!rendered.contains("}}"));
    }

    #[test]
    fn test_render_overlay_empty_file_scope() {
        let mut config = base_config();
        config.file_scope = vec![];
        let rendered = render_overlay(SIMPLE_TEMPLATE, &config);
        assert!(rendered.contains("No file scope restrictions"));
    }

    #[test]
    fn test_render_overlay_no_spec() {
        let mut config = base_config();
        config.spec_path = None;
        let rendered = render_overlay(SIMPLE_TEMPLATE, &config);
        assert!(rendered.contains("No spec file provided"));
    }

    #[test]
    fn test_render_overlay_with_quality_gates() {
        let mut config = base_config();
        config.quality_gates = Some(vec![
            QualityGate {
                name: "Tests".to_string(),
                command: "cargo test".to_string(),
                description: "all tests pass".to_string(),
            },
            QualityGate {
                name: "Lint".to_string(),
                command: "cargo clippy".to_string(),
                description: "no warnings".to_string(),
            },
        ]);
        let rendered = render_overlay(SIMPLE_TEMPLATE, &config);
        assert!(rendered.contains("1. **Tests:**"));
        assert!(rendered.contains("2. **Lint:**"));
        // Numbered commit step follows
        assert!(rendered.contains("3. **Commit:**"));
    }
}
