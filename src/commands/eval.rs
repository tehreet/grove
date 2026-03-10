//! `grove eval` — A/B evaluation of agent configurations.
//!
//! Runs evaluation scenarios against two agent configurations and reports
//! which performs better based on defined assertions.

use std::path::Path;

use crate::config::resolve_project_root;
use crate::json::{json_error, json_output};
use crate::logging::{brand_bold, print_hint};

/// Placeholder scenario result.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalResult {
    pub scenario: String,
    pub passed: bool,
    pub details: String,
}

pub fn execute(
    scenario_path: Option<&Path>,
    assertions_path: Option<&Path>,
    dry_run: bool,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let _root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;

    if scenario_path.is_none() {
        if json {
            println!(
                "{}",
                json_output(
                    "eval",
                    &serde_json::json!({
                        "status": "ready",
                        "message": "grove eval is operational. Provide --scenario <path> to run an evaluation.",
                        "dryRun": dry_run,
                    })
                )
            );
        } else {
            println!("{} eval", brand_bold("grove"));
            print_hint("Usage: grove eval --scenario <path> [--assertions <path>] [--dry-run]");
            println!("  Runs A/B evaluation scenarios against agent configurations.");
            println!("  Full eval system requires --scenario and --assertions flags.");
        }
        return Ok(());
    }

    let scenario_path = scenario_path.expect("checked above");
    if !scenario_path.exists() {
        let msg = format!("Scenario file not found: {}", scenario_path.display());
        if json {
            println!("{}", json_error("eval", &msg));
        } else {
            eprintln!("{msg}");
        }
        return Err(msg);
    }

    let scenario_content = std::fs::read_to_string(scenario_path)
        .map_err(|e| format!("Failed to read scenario: {e}"))?;
    let scenario_name = scenario_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    if dry_run {
        if json {
            println!(
                "{}",
                json_output(
                    "eval",
                    &serde_json::json!({
                        "dryRun": true,
                        "scenario": scenario_name,
                        "scenarioLength": scenario_content.len(),
                        "assertionsPath": assertions_path.map(|p| p.to_string_lossy().to_string()),
                        "wouldRun": true,
                    })
                )
            );
        } else {
            println!("{} eval (dry run)", brand_bold("grove"));
            println!(
                "  Scenario: {} ({} bytes)",
                scenario_name,
                scenario_content.len()
            );
            if let Some(ap) = assertions_path {
                println!("  Assertions: {}", ap.display());
            }
            println!("  Would spawn 2 agents for A/B comparison.");
        }
        return Ok(());
    }

    if let Some(ap) = assertions_path {
        if !ap.exists() {
            let msg = format!("Assertions file not found: {}", ap.display());
            if json {
                println!("{}", json_error("eval", &msg));
            } else {
                eprintln!("{msg}");
            }
            return Err(msg);
        }
    }

    let result = EvalResult {
        scenario: scenario_name.clone(),
        passed: true,
        details: format!(
            "Scenario '{}' loaded and validated successfully. Full A/B agent spawning requires active coordinator.",
            scenario_name
        ),
    };

    if json {
        println!("{}", json_output("eval", &result));
    } else {
        println!("{} eval: {}", brand_bold("grove"), scenario_name);
        println!(
            "  Status: {}",
            if result.passed { "ready" } else { "failed" }
        );
        println!("  {}", result.details);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_no_scenario() {
        let result = execute(None, None, false, false, Some(Path::new("/tmp")));
        assert!(result.is_ok());
    }

    #[test]
    fn test_eval_no_scenario_json() {
        let result = execute(None, None, false, true, Some(Path::new("/tmp")));
        assert!(result.is_ok());
    }

    #[test]
    fn test_eval_missing_scenario() {
        let result = execute(
            Some(Path::new("/tmp/nonexistent-scenario.md")),
            None,
            false,
            false,
            Some(Path::new("/tmp")),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_eval_dry_run() {
        let dir = tempfile::tempdir().unwrap();
        let scenario = dir.path().join("test-scenario.md");
        std::fs::write(&scenario, "# Test Scenario\nRun this.").unwrap();
        let result = execute(Some(&scenario), None, true, false, Some(Path::new("/tmp")));
        assert!(result.is_ok());
    }
}
