#![allow(dead_code)]

use std::path::Path;

use crate::types::AgentManifest;

/// Load an agent manifest from a file path.
pub fn load_manifest(path: &Path) -> Result<AgentManifest, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read manifest at {}: {e}", path.display()))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse manifest JSON: {e}"))
}

/// Load an agent manifest relative to a project root using a relative path.
pub fn load_manifest_from_project(
    project_root: &Path,
    manifest_path: &str,
) -> Result<AgentManifest, String> {
    let full_path = project_root.join(manifest_path);
    load_manifest(&full_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest_json() -> &'static str {
        r#"{
            "version": "1.0",
            "agents": {
                "builder": {
                    "file": "agents/builder.md",
                    "model": "claude-opus-4-6",
                    "tools": ["Read", "Write"],
                    "capabilities": ["builder"],
                    "canSpawn": false,
                    "constraints": []
                }
            },
            "capabilityIndex": {
                "builder": ["builder"]
            }
        }"#
    }

    #[test]
    fn test_load_manifest_valid() {
        let manifest: AgentManifest = serde_json::from_str(sample_manifest_json()).unwrap();
        assert_eq!(manifest.version, "1.0");
        assert!(manifest.agents.contains_key("builder"));
    }

    #[test]
    fn test_load_manifest_invalid_json() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut f = NamedTempFile::new().unwrap();
        write!(f, "not valid json {{{{").unwrap();
        let result = load_manifest(f.path());
        assert!(result.is_err());
    }
}
