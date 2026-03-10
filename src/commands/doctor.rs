#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::config::load_config;
use crate::json::json_output;
use crate::logging::{
    color_dim, color_green, color_red, color_yellow, render_header, separator,
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DoctorSummary {
    passed: usize,
    warnings: usize,
    failed: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DoctorOutput {
    checks: Vec<DoctorCheck>,
    summary: DoctorSummary,
}

// ---------------------------------------------------------------------------
// which helper
// ---------------------------------------------------------------------------

fn which_command(name: &str) -> Option<String> {
    Command::new("which")
        .arg(name)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

// ---------------------------------------------------------------------------
// Individual checks
// ---------------------------------------------------------------------------

fn check_git() -> DoctorCheck {
    let name = "git".to_string();
    let category = Some("dependencies".to_string());

    let output = Command::new("git").arg("--version").output();
    match output {
        Err(_) => DoctorCheck {
            name,
            status: CheckStatus::Fail,
            detail: "git not found".to_string(),
            category,
        },
        Ok(o) if !o.status.success() => DoctorCheck {
            name,
            status: CheckStatus::Fail,
            detail: "git --version failed".to_string(),
            category,
        },
        Ok(o) => {
            let version_str = String::from_utf8_lossy(&o.stdout);
            // "git version 2.43.0" → extract "2.43.0"
            let version = version_str
                .split_whitespace()
                .last()
                .unwrap_or("")
                .to_string();

            // Parse major.minor
            let parts: Vec<u32> = version
                .split('.')
                .take(2)
                .filter_map(|p| p.parse().ok())
                .collect();
            let (major, minor) = (
                parts.first().copied().unwrap_or(0),
                parts.get(1).copied().unwrap_or(0),
            );
            let passes = major > 2 || (major == 2 && minor >= 20);

            if passes {
                DoctorCheck {
                    name,
                    status: CheckStatus::Pass,
                    detail: version,
                    category,
                }
            } else {
                DoctorCheck {
                    name,
                    status: CheckStatus::Fail,
                    detail: format!("{version} (requires >= 2.20)"),
                    category,
                }
            }
        }
    }
}


fn check_agent_runtime(runtime_name: &str) -> DoctorCheck {
    let category = Some("dependencies".to_string());
    match which_command(runtime_name) {
        Some(_) => DoctorCheck {
            name: runtime_name.to_string(),
            status: CheckStatus::Pass,
            detail: "runtime".to_string(),
            category,
        },
        None => DoctorCheck {
            name: runtime_name.to_string(),
            status: CheckStatus::Fail,
            detail: format!("{runtime_name} not found in PATH"),
            category,
        },
    }
}

fn check_overstory_dir(overstory_dir: &Path) -> DoctorCheck {
    let category = Some("structure".to_string());
    if overstory_dir.is_dir() {
        DoctorCheck {
            name: ".overstory/ directory".to_string(),
            status: CheckStatus::Pass,
            detail: overstory_dir.display().to_string(),
            category,
        }
    } else {
        DoctorCheck {
            name: ".overstory/ directory".to_string(),
            status: CheckStatus::Fail,
            detail: "not found".to_string(),
            category,
        }
    }
}

fn check_config_yaml(overstory_dir: &Path) -> DoctorCheck {
    let category = Some("config".to_string());
    let config_path = overstory_dir.join("config.yaml");
    if config_path.exists() {
        DoctorCheck {
            name: "config.yaml".to_string(),
            status: CheckStatus::Pass,
            detail: config_path.display().to_string(),
            category,
        }
    } else {
        DoctorCheck {
            name: "config.yaml".to_string(),
            status: CheckStatus::Fail,
            detail: "not found".to_string(),
            category,
        }
    }
}

fn check_database(overstory_dir: &Path, db_name: &str) -> DoctorCheck {
    let category = Some("databases".to_string());
    let db_path = overstory_dir.join(db_name);
    if db_path.exists() {
        DoctorCheck {
            name: db_name.to_string(),
            status: CheckStatus::Pass,
            detail: "exists".to_string(),
            category,
        }
    } else {
        DoctorCheck {
            name: db_name.to_string(),
            status: CheckStatus::Warn,
            detail: "not yet created".to_string(),
            category,
        }
    }
}

fn check_quality_gate(gate_name: &str, gate_command: &str) -> DoctorCheck {
    let category = Some("config".to_string());
    let binary = gate_command.split_whitespace().next().unwrap_or("");
    if binary.is_empty() {
        return DoctorCheck {
            name: format!("quality gate: {gate_name}"),
            status: CheckStatus::Warn,
            detail: "empty command".to_string(),
            category,
        };
    }
    match which_command(binary) {
        Some(_) => DoctorCheck {
            name: format!("quality gate: {gate_name}"),
            status: CheckStatus::Pass,
            detail: format!("{binary} found"),
            category,
        },
        None => DoctorCheck {
            name: format!("quality gate: {gate_name}"),
            status: CheckStatus::Warn,
            detail: format!("{binary} not found in PATH"),
            category,
        },
    }
}

fn check_agent_manifest(project_root: &Path, manifest_path: &str) -> DoctorCheck {
    let category = Some("structure".to_string());
    let full_path = project_root.join(manifest_path);

    if !full_path.exists() {
        return DoctorCheck {
            name: "agent manifest".to_string(),
            status: CheckStatus::Warn,
            detail: "not found".to_string(),
            category,
        };
    }

    match std::fs::read_to_string(&full_path) {
        Err(e) => DoctorCheck {
            name: "agent manifest".to_string(),
            status: CheckStatus::Fail,
            detail: format!("read error: {e}"),
            category,
        },
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Err(e) => DoctorCheck {
                name: "agent manifest".to_string(),
                status: CheckStatus::Fail,
                detail: format!("parse error: {e}"),
                category,
            },
            Ok(json) => {
                let count = json.as_array().map(|a| a.len()).unwrap_or(0);
                DoctorCheck {
                    name: "agent manifest".to_string(),
                    status: CheckStatus::Pass,
                    detail: format!("{count} agents"),
                    category,
                }
            }
        },
    }
}

// ---------------------------------------------------------------------------
// Public execute function
// ---------------------------------------------------------------------------

pub fn execute(json: bool, _verbose: bool, category: Option<String>) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| format!("Failed to get cwd: {e}"))?;
    let config = load_config(&cwd, None).map_err(|e| e.to_string())?;

    let project_root = Path::new(&config.project.root);
    let overstory_dir = project_root.join(".overstory");

    let runtime_name = config
        .runtime
        .as_ref()
        .map(|r| r.default.as_str())
        .unwrap_or("claude")
        .to_string();

    let mut checks = vec![
        check_git(),
        check_agent_runtime(&runtime_name),
        check_overstory_dir(&overstory_dir),
        check_config_yaml(&overstory_dir),
    ];

    for db in &["sessions.db", "mail.db", "events.db", "metrics.db", "merge-queue.db"] {
        checks.push(check_database(&overstory_dir, db));
    }

    if let Some(gates) = &config.project.quality_gates {
        for gate in gates {
            checks.push(check_quality_gate(&gate.name, &gate.command));
        }
    }

    checks.push(check_agent_manifest(project_root, &config.agents.manifest_path));

    // Apply category filter
    if let Some(ref cat) = category {
        checks.retain(|c| c.category.as_deref() == Some(cat.as_str()));
    }

    let passed = checks.iter().filter(|c| c.status == CheckStatus::Pass).count();
    let warnings = checks.iter().filter(|c| c.status == CheckStatus::Warn).count();
    let failed = checks.iter().filter(|c| c.status == CheckStatus::Fail).count();

    if json {
        let output = DoctorOutput {
            checks,
            summary: DoctorSummary { passed, warnings, failed },
        };
        println!("{}", json_output("doctor", &output));
    } else {
        println!("{}", render_header("Grove Doctor", None));
        for check in &checks {
            let icon = match check.status {
                CheckStatus::Pass => color_green("✓").to_string(),
                CheckStatus::Warn => color_yellow("⚠").to_string(),
                CheckStatus::Fail => color_red("✗").to_string(),
            };
            let detail = if check.detail.is_empty() {
                String::new()
            } else {
                format!(" {}", color_dim(&format!("({})", check.detail)))
            };
            println!("  {icon} {}{detail}", check.name);
        }
        println!("{}", separator(None));
        println!(
            "  {} passed, {} warning{}, {} failed",
            passed,
            warnings,
            if warnings == 1 { "" } else { "s" },
            failed
        );
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
    fn check_git_returns_pass_or_fail() {
        let result = check_git();
        // On a dev machine git should be installed and >= 2.20
        assert!(
            result.status == CheckStatus::Pass || result.status == CheckStatus::Fail,
            "Expected pass or fail, got warn"
        );
    }

    
    #[test]
    fn check_database_missing() {
        let dir = TempDir::new().unwrap();
        let result = check_database(dir.path(), "sessions.db");
        assert_eq!(result.status, CheckStatus::Warn);
        assert!(result.detail.contains("not yet created"));
    }

    #[test]
    fn check_database_present() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("sessions.db"), b"").unwrap();
        let result = check_database(dir.path(), "sessions.db");
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn check_overstory_dir_missing() {
        let result = check_overstory_dir(Path::new("/nonexistent/path/.overstory"));
        assert_eq!(result.status, CheckStatus::Fail);
    }

    #[test]
    fn check_quality_gate_with_known_binary() {
        let result = check_quality_gate("Git Check", "git status");
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn check_quality_gate_with_unknown_binary() {
        let result = check_quality_gate("Unknown", "nonexistent-xyz-binary --test");
        assert_eq!(result.status, CheckStatus::Warn);
    }

    #[test]
    fn check_status_serialization() {
        assert_eq!(
            serde_json::to_string(&CheckStatus::Pass).unwrap(),
            "\"pass\""
        );
        assert_eq!(
            serde_json::to_string(&CheckStatus::Warn).unwrap(),
            "\"warn\""
        );
        assert_eq!(
            serde_json::to_string(&CheckStatus::Fail).unwrap(),
            "\"fail\""
        );
    }

    #[test]
    fn doctor_check_json_format() {
        let check = DoctorCheck {
            name: "git".to_string(),
            status: CheckStatus::Pass,
            detail: "2.43.0".to_string(),
            category: Some("dependencies".to_string()),
        };
        let json: serde_json::Value = serde_json::from_str(&serde_json::to_string(&check).unwrap()).unwrap();
        assert_eq!(json["name"], "git");
        assert_eq!(json["status"], "pass");
        assert_eq!(json["detail"], "2.43.0");
        assert_eq!(json["category"], "dependencies");
    }

    #[test]
    fn which_command_finds_git() {
        let result = which_command("git");
        assert!(result.is_some(), "git should be findable via which");
    }

    #[test]
    fn which_command_missing_binary() {
        let result = which_command("nonexistent-binary-xyz-12345");
        assert!(result.is_none());
    }
}
