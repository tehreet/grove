#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::json::json_output;
use crate::logging::{accent, brand_bold, color_dim, color_red, muted, thick_separator};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolInfo {
    name: String,
    cli: String,
    installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    doctor_summary: Option<DoctorSummaryInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DoctorSummaryInfo {
    passed: usize,
    warnings: usize,
    failed: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EcoSummary {
    total: usize,
    installed: usize,
    missing: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EcosystemOutput {
    tools: Vec<ToolInfo>,
    summary: EcoSummary,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract a semver string from command output using regex-like matching.
pub fn extract_version(output: &str) -> Option<String> {
    // Find pattern: digits.digits.digits
    for part in output.split_whitespace() {
        let chars: Vec<char> = part.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i].is_ascii_digit() {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let candidate: String = chars[start..i].iter().collect();
                // Must match X.Y.Z pattern (at least one dot)
                let parts: Vec<&str> = candidate.split('.').collect();
                if parts.len() >= 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
                    return Some(candidate);
                }
            } else {
                i += 1;
            }
        }
    }
    None
}

/// Run a binary with --version, return extracted version or None if not found.
pub fn get_tool_version(bin: &str) -> Option<String> {
    let output = Command::new(bin).arg("--version").output().ok()?;
    if !output.status.success() && output.stdout.is_empty() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);
    extract_version(&combined)
}

fn get_doctor_summary(bin: &str) -> Option<DoctorSummaryInfo> {
    let output = Command::new(bin)
        .args(["doctor", "--json"])
        .output()
        .ok()?;
    if !output.status.success() && output.stdout.is_empty() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).ok()?;
    let passed = v["summary"]["passed"].as_u64()? as usize;
    let warnings = v["summary"]["warnings"].as_u64().unwrap_or(0) as usize;
    let failed = v["summary"]["failed"].as_u64().unwrap_or(0) as usize;
    Some(DoctorSummaryInfo { passed, warnings, failed })
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

struct ToolDef {
    display_name: &'static str,
    cli_alias: &'static str,
    primary_bin: &'static str,
    fallback_bin: Option<&'static str>,
    has_doctor: bool,
}

fn tool_definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            display_name: "overstory (ov) / grove",
            cli_alias: "ov / grove",
            primary_bin: "ov",
            fallback_bin: Some("grove"),
            has_doctor: true,
        },
        ToolDef {
            display_name: "mulch (ml)",
            cli_alias: "ml",
            primary_bin: "ml",
            fallback_bin: None,
            has_doctor: false,
        },
        ToolDef {
            display_name: "seeds (sd)",
            cli_alias: "sd",
            primary_bin: "sd",
            fallback_bin: None,
            has_doctor: false,
        },
        ToolDef {
            display_name: "canopy (cn)",
            cli_alias: "cn",
            primary_bin: "cn",
            fallback_bin: None,
            has_doctor: false,
        },
    ]
}

fn check_tool(def: &ToolDef) -> ToolInfo {
    // Try primary bin first, then fallback
    let version = get_tool_version(def.primary_bin)
        .or_else(|| def.fallback_bin.and_then(|fb| get_tool_version(fb)));

    let installed = version.is_some();

    // Try doctor summary for overstory
    let doctor_summary = if installed && def.has_doctor {
        get_doctor_summary(def.primary_bin)
            .or_else(|| def.fallback_bin.and_then(|fb| get_doctor_summary(fb)))
    } else {
        None
    };

    ToolInfo {
        name: def.display_name.to_string(),
        cli: def.cli_alias.to_string(),
        installed,
        version,
        doctor_summary,
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn execute(json: bool, _project_override: Option<&Path>) -> Result<(), String> {
    let defs = tool_definitions();
    let tools: Vec<ToolInfo> = defs.iter().map(check_tool).collect();

    let total = tools.len();
    let installed = tools.iter().filter(|t| t.installed).count();
    let missing = total - installed;

    if json {
        let summary = EcoSummary { total, installed, missing };
        let output = EcosystemOutput { tools, summary };
        println!("{}", json_output("ecosystem", &output));
        return Ok(());
    }

    // Human output
    println!("{}", brand_bold("os-eco Ecosystem"));
    println!("{}", thick_separator(None));
    println!();

    for tool in &tools {
        println!("  - {}", accent(&tool.name));
        if tool.installed {
            if let Some(ref ver) = tool.version {
                println!("    {}", color_dim(&format!("Version: {}", ver)));
            }
            if let Some(ref doc) = tool.doctor_summary {
                println!(
                    "    {}",
                    color_dim(&format!(
                        "Doctor:  {} passed, {} warn",
                        doc.passed, doc.warnings
                    ))
                );
                if doc.failed > 0 {
                    println!(
                        "    {}",
                        color_dim(&format!("         {} failed", doc.failed))
                    );
                }
            }
        } else {
            println!("    {}", color_red("not installed"));
        }
        println!();
    }

    // Footer summary
    println!(
        "{}",
        muted(&format!(
            "{} tools checked: {} installed, {} missing",
            total, installed, missing
        ))
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_version_from_string() {
        assert_eq!(
            extract_version("mulch 0.6.3 (build 2026)"),
            Some("0.6.3".into())
        );
        assert_eq!(
            extract_version("grove version 0.1.0"),
            Some("0.1.0".into())
        );
        assert_eq!(extract_version("no version here"), None);
    }

    #[test]
    fn test_extract_version_bare_semver() {
        assert_eq!(extract_version("1.2.3"), Some("1.2.3".into()));
        assert_eq!(extract_version("v0.2.5"), Some("0.2.5".into()));
    }

    #[test]
    fn test_get_tool_version_known_binary() {
        // git is always available in dev environments
        let ver = get_tool_version("git");
        assert!(ver.is_some(), "expected git version to be found");
    }

    #[test]
    fn test_get_tool_version_missing_binary() {
        let ver = get_tool_version("this-binary-does-not-exist-xyz");
        assert!(ver.is_none());
    }

    #[test]
    fn test_ecosystem_output_json_structure() {
        let output = EcosystemOutput {
            tools: vec![
                ToolInfo {
                    name: "mulch (ml)".into(),
                    cli: "ml".into(),
                    installed: true,
                    version: Some("0.6.3".into()),
                    doctor_summary: None,
                },
                ToolInfo {
                    name: "seeds (sd)".into(),
                    cli: "sd".into(),
                    installed: false,
                    version: None,
                    doctor_summary: None,
                },
            ],
            summary: EcoSummary {
                total: 2,
                installed: 1,
                missing: 1,
            },
        };
        let json_str = json_output("ecosystem", &output);
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["success"], true);
        assert_eq!(v["command"], "ecosystem");
        assert_eq!(v["summary"]["total"], 2);
        assert_eq!(v["summary"]["installed"], 1);
        assert_eq!(v["tools"][0]["installed"], true);
        assert_eq!(v["tools"][0]["version"], "0.6.3");
        assert_eq!(v["tools"][1]["installed"], false);
        // version should be absent when None
        assert!(v["tools"][1].get("version").is_none() || v["tools"][1]["version"].is_null());
    }
}
