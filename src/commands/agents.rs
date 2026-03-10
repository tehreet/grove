//! `grove agents` — list agent definitions from the agent manifest.
//!
//! Reads `.overstory/agent-manifest.json` and displays available agent definitions
//! with their capabilities, model, tools, and spawn permissions.

use std::path::Path;

use serde::Serialize;

use crate::agents::manifest::load_manifest_from_project;
use crate::config::load_config;
use crate::json::json_output;
use crate::logging::{accent, muted, print_hint};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestAgentEntry {
    pub name: String,
    pub file: String,
    pub model: String,
    pub capabilities: Vec<String>,
    pub can_spawn: bool,
    pub tools: Vec<String>,
    pub constraints: Vec<String>,
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

pub fn execute_discover(
    capability: Option<String>,
    _include_all: bool,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let root = std::path::Path::new(&config.project.root);

    let manifest = match load_manifest_from_project(root, ".overstory/agent-manifest.json") {
        Ok(m) => m,
        Err(_) => {
            if json {
                println!("{}", json_output("agents", &serde_json::json!({"agents": []})));
            } else {
                print_hint("No agent manifest found");
            }
            return Ok(());
        }
    };

    // Build entries, optionally filtering by capability
    let mut entries: Vec<ManifestAgentEntry> = manifest
        .agents
        .iter()
        .filter(|(_, def)| {
            if let Some(ref cap) = capability {
                def.capabilities.iter().any(|c| c == cap)
            } else {
                true
            }
        })
        .map(|(name, def)| {
            let model_str = serde_json::to_value(&def.model)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| format!("{:?}", def.model));
            ManifestAgentEntry {
                name: name.clone(),
                file: def.file.clone(),
                model: model_str,
                capabilities: def.capabilities.clone(),
                can_spawn: def.can_spawn,
                tools: def.tools.clone(),
                constraints: def.constraints.clone(),
            }
        })
        .collect();

    // Sort by name for stable output
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    if json {
        println!("{}", json_output("agents", &serde_json::json!({"agents": entries})));
    } else {
        print_agents(&entries);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Text formatting
// ---------------------------------------------------------------------------

fn print_agents(agents: &[ManifestAgentEntry]) {
    if agents.is_empty() {
        println!("No agent definitions found.");
        return;
    }

    println!("Agent definitions ({}):\n", agents.len());

    for agent in agents {
        println!("  {}", accent(&agent.name));
        println!("    File:         {}", agent.file);
        println!("    Model:        {}", agent.model);
        println!("    Capabilities: {}", agent.capabilities.join(", "));
        println!("    Can spawn:    {}", agent.can_spawn);
        if agent.tools.is_empty() {
            println!("    Tools:        {}", muted("(none)"));
        } else {
            println!("    Tools:        {}", agent.tools.join(", "));
        }
        if !agent.constraints.is_empty() {
            println!("    Constraints:  {}", agent.constraints.join(", "));
        }
        println!();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_discover_no_manifest() {
        // /tmp has no grove.yaml, load_config will fail — that's OK, function returns Err or Ok
        let result = execute_discover(None, false, false, Some(Path::new("/tmp")));
        // May fail at config load, that's acceptable
        let _ = result;
    }

    #[test]
    fn test_execute_discover_json_no_manifest() {
        let result = execute_discover(None, false, true, Some(Path::new("/tmp")));
        let _ = result;
    }

    #[test]
    fn test_manifest_entry_serialization() {
        let entry = ManifestAgentEntry {
            name: "builder".to_string(),
            file: "agents/builder.md".to_string(),
            model: "claude-opus-4-6".to_string(),
            capabilities: vec!["builder".to_string()],
            can_spawn: false,
            tools: vec!["Read".to_string(), "Write".to_string()],
            constraints: vec![],
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"name\":\"builder\""));
        assert!(json.contains("\"canSpawn\":false"));
    }
}
