fn main() {
    // Rerun if git metadata changes
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
    println!("cargo:rerun-if-changed=agents/");

    let pkg_version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".to_string());

    // Try git tag first, fall back to Cargo.toml version + commit hash
    let git_tag = std::process::Command::new("git")
        .args(["describe", "--tags", "--exact-match"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        });

    let git_commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string());

    let grove_version = match git_tag {
        Some(tag) => format!("{} ({})", tag.trim_start_matches('v'), git_commit),
        None => format!("{} ({})", pkg_version, git_commit),
    };

    println!("cargo:rustc-env=GROVE_VERSION={grove_version}");
    println!("cargo:rustc-env=GROVE_COMMIT={git_commit}");

    // Build timestamp as UNIX seconds (formatted by main using chrono)
    let build_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    println!("cargo:rustc-env=GROVE_BUILD_TIME={build_secs}");
}
