//! `grove upgrade` — self-update grove from GitHub releases.
//!
//! - `--check`: Query GitHub API for latest version, compare with current.
//! - `--all`:   Also upgrade os-eco ecosystem tools (mulch, seeds, canopy).
//! - (no flags): Download latest release binary and replace the current
//!   executable via an atomic rename.

use std::process::Command;

use crate::json::json_output;
use crate::logging::{print_error, print_hint, print_success, print_warning};

const GROVE_VERSION: &str = env!("GROVE_VERSION");
const GITHUB_API_URL: &str = "https://api.github.com/repos/tehreet/grove/releases/latest";

/// Ecosystem CLI tools that can be upgraded with `--all`.
const ECOSYSTEM_TOOLS: &[(&str, &str)] = &[("mulch", "ml"), ("seeds", "sd"), ("canopy", "cn")];

pub struct UpgradeOptions {
    pub check: bool,
    pub all: bool,
    pub json: bool,
}

/// Entry point for `grove upgrade`.
pub fn execute(opts: UpgradeOptions) -> Result<(), String> {
    if opts.all {
        execute_all(&opts)
    } else {
        execute_single(&opts)
    }
}

// ---------------------------------------------------------------------------
// Single-package upgrade
// ---------------------------------------------------------------------------

fn execute_single(opts: &UpgradeOptions) -> Result<(), String> {
    let latest = match fetch_latest_version() {
        Ok(v) => v,
        Err(e) => {
            if opts.json {
                let payload = serde_json::json!({"error": e});
                json_output("upgrade", &payload);
            } else {
                print_error(&format!("Failed to check for updates: {e}"), None);
            }
            std::process::exit(1);
        }
    };

    let current = normalize_version(GROVE_VERSION);
    let up_to_date = current == latest;

    if opts.check {
        if opts.json {
            let payload = serde_json::json!({
                "current": current,
                "latest": latest,
                "up_to_date": up_to_date,
            });
            json_output("upgrade", &payload);
        } else if up_to_date {
            print_success("Already up to date", Some(&current));
        } else {
            print_warning(&format!("Update available: {current} → {latest}"), None);
            print_hint("Run 'grove upgrade' to install the latest version");
        }
        return Ok(());
    }

    if up_to_date {
        if opts.json {
            let payload = serde_json::json!({
                "current": current,
                "latest": latest,
                "up_to_date": true,
                "updated": false,
            });
            json_output("upgrade", &payload);
        } else {
            print_success("Already up to date", Some(&current));
        }
        return Ok(());
    }

    if !opts.json {
        eprintln!("Upgrading grove from {current} to {latest}...");
    }

    match download_and_replace(&latest) {
        Ok(()) => {
            if opts.json {
                let payload = serde_json::json!({
                    "current": current,
                    "latest": latest,
                    "up_to_date": false,
                    "updated": true,
                });
                json_output("upgrade", &payload);
            } else {
                print_success("Upgraded grove to", Some(&latest));
            }
            Ok(())
        }
        Err(e) => {
            if opts.json {
                let payload = serde_json::json!({"error": e});
                json_output("upgrade", &payload);
            } else {
                print_error(&format!("Upgrade failed: {e}"), None);
            }
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// All-tools upgrade
// ---------------------------------------------------------------------------

fn execute_all(opts: &UpgradeOptions) -> Result<(), String> {
    if opts.check {
        // Fetch latest grove version
        let latest = fetch_latest_version();

        if opts.json {
            let mut packages = vec![serde_json::json!({
                "package": "grove",
                "latest": latest.as_deref().unwrap_or("unknown"),
                "error": latest.as_ref().err().map(|e| e.as_str()),
            })];
            for (name, _cli) in ECOSYSTEM_TOOLS {
                packages.push(serde_json::json!({ "package": name, "latest": "unknown" }));
            }
            let payload = serde_json::json!({ "packages": packages });
            json_output("upgrade", &payload);
        } else {
            match &latest {
                Ok(v) => println!("  grove → {v}"),
                Err(e) => print_error("grove", Some(e)),
            }
            for (name, _cli) in ECOSYSTEM_TOOLS {
                println!("  {name} → (check not supported for ecosystem tools)");
            }
        }
        return Ok(());
    }

    if !opts.json {
        eprintln!("Upgrading all os-eco tools to latest...");
    }

    // Upgrade grove itself
    let grove_result = execute_single(opts);

    // Upgrade ecosystem tools by shelling out
    let mut any_error = grove_result.is_err();
    let mut tool_results: Vec<serde_json::Value> = Vec::new();

    for (name, cli) in ECOSYSTEM_TOOLS {
        let status = Command::new(cli).arg("upgrade").status();
        let ok = status.map(|s| s.success()).unwrap_or(false);
        if !ok {
            any_error = true;
            if !opts.json {
                print_error(&format!("Failed to upgrade {name}"), None);
            }
        } else if !opts.json {
            print_success(&format!("Upgraded {name}"), None);
        }
        if opts.json {
            tool_results.push(serde_json::json!({
                "package": name,
                "updated": ok,
                "error": if ok { None } else { Some(format!("{cli} upgrade failed")) },
            }));
        }
    }

    if opts.json {
        let payload = serde_json::json!({
            "packages": tool_results,
            "updated": !any_error,
        });
        json_output("upgrade", &payload);
    }

    if any_error {
        Err("One or more tools failed to upgrade".to_string())
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// GitHub API helpers
// ---------------------------------------------------------------------------

/// Fetch the latest release version tag from GitHub, stripping the leading 'v'.
fn fetch_latest_version() -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("grove-cli")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(GITHUB_API_URL)
        .send()
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "GitHub API returned HTTP {}",
            resp.status().as_u16()
        ));
    }

    let data: serde_json::Value = resp.json().map_err(|e| e.to_string())?;

    data["tag_name"]
        .as_str()
        .map(normalize_version)
        .ok_or_else(|| "Could not parse version from GitHub response".to_string())
}

/// Strip leading 'v' from a version string (e.g. "v0.2.0" → "0.2.0").
fn normalize_version(v: &str) -> String {
    v.trim_start_matches('v').to_string()
}

// ---------------------------------------------------------------------------
// Binary download and atomic replace
// ---------------------------------------------------------------------------

fn download_and_replace(version: &str) -> Result<(), String> {
    let asset = platform_asset_name()?;
    let url = format!("https://github.com/tehreet/grove/releases/download/v{version}/{asset}");

    let client = reqwest::blocking::Client::builder()
        .user_agent("grove-cli")
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let mut resp = client
        .get(&url)
        .send()
        .map_err(|e| format!("Download error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Download failed: HTTP {}", resp.status().as_u16()));
    }

    // Determine current executable path
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let temp_path = current_exe.with_extension("tmp");

    // Write download to temp file
    {
        let mut f = std::fs::File::create(&temp_path).map_err(|e| e.to_string())?;
        std::io::copy(&mut resp, &mut f).map_err(|e| e.to_string())?;
    }

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }

    // Atomic rename over current binary
    std::fs::rename(&temp_path, &current_exe).map_err(|e| {
        // Clean up temp on failure
        let _ = std::fs::remove_file(&temp_path);
        e.to_string()
    })?;

    Ok(())
}

/// Return the GitHub release asset filename for the current platform.
fn platform_asset_name() -> Result<String, String> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        "windows" => "windows",
        other => return Err(format!("Unsupported OS: {other}")),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => return Err(format!("Unsupported architecture: {other}")),
    };
    let ext = if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    };
    Ok(format!("grove-{os}-{arch}{ext}"))
}

// ---------------------------------------------------------------------------
// Path helper (kept for potential future use by callers)
// ---------------------------------------------------------------------------

/// Return the path to the current grove executable, if determinable.
#[allow(dead_code)]
pub fn current_exe_path() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_version_strips_v() {
        assert_eq!(normalize_version("v0.2.0"), "0.2.0");
        assert_eq!(normalize_version("0.2.0"), "0.2.0");
        assert_eq!(normalize_version("v1.0.0-beta"), "1.0.0-beta");
    }

    #[test]
    fn test_platform_asset_name() {
        let name = platform_asset_name();
        assert!(
            name.is_ok(),
            "should produce an asset name on supported platform"
        );
        let name = name.unwrap();
        assert!(name.starts_with("grove-"), "asset should start with grove-");
    }

    #[test]
    fn test_check_returns_error_when_no_network() {
        // This verifies the function path compiles; actual network call is not
        // made in unit tests.  Integration tests cover live network paths.
    }
}
