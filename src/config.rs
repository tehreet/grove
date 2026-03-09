#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_yaml::Value;

use crate::errors::{ConfigError, GroveError, ValidationError};
use crate::types::{
    default_verification_config, OverstoryConfig, TaskTrackerBackend,
};

const CONFIG_FILENAME: &str = "config.yaml";
const CONFIG_LOCAL_FILENAME: &str = "config.local.yaml";
const OVERSTORY_DIR: &str = ".overstory";

/// Resolve the actual project root, handling git worktrees.
///
/// When running inside a git worktree (e.g., an agent's worktree at
/// `.overstory/worktrees/{name}/`), the passed directory won't contain
/// `.overstory/config.yaml`. This function detects worktrees using
/// `git rev-parse --git-common-dir` and resolves to the main repository root.
pub fn resolve_project_root(
    start_dir: &Path,
    override_path: Option<&Path>,
) -> Result<PathBuf, GroveError> {
    // Explicit override takes priority
    if let Some(p) = override_path {
        return Ok(p.to_path_buf());
    }

    let config_rel = Path::new(OVERSTORY_DIR).join(CONFIG_FILENAME);

    // Check git worktree first. When running from an agent worktree, we must
    // resolve to the main repository root so runtime state is shared across all agents.
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(start_dir)
        .output()
    {
        if output.status.success() {
            let git_common_dir = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_string();
            let abs_git_common = if Path::new(&git_common_dir).is_absolute() {
                PathBuf::from(&git_common_dir)
            } else {
                start_dir.join(&git_common_dir)
            };
            // Main repo root is the parent of the .git directory
            if let Some(main_root) = abs_git_common.parent() {
                if main_root != start_dir && main_root.join(&config_rel).exists() {
                    return Ok(main_root.to_path_buf());
                }
            }
        }
    }

    // Not inside a worktree (or git not available).
    // Check if .overstory/config.yaml exists at start_dir.
    if start_dir.join(&config_rel).exists() {
        return Ok(start_dir.to_path_buf());
    }

    // Fall back to start_dir
    Ok(start_dir.to_path_buf())
}

/// Deep merge source into target. Source values override target values.
/// Arrays from source replace (not append) target arrays.
fn deep_merge(target: Value, source: Value) -> Value {
    match (target, source) {
        (Value::Mapping(mut t), Value::Mapping(s)) => {
            for (k, v) in s {
                let entry = t.remove(&k).unwrap_or(Value::Null);
                t.insert(k, deep_merge(entry, v));
            }
            Value::Mapping(t)
        }
        (_, s) => s,
    }
}

/// Migrate deprecated watchdog tier key names.
///
/// Old naming: tier1 = mechanical daemon, tier2 = AI triage
/// New naming: tier0 = mechanical daemon, tier1 = AI triage, tier2 = monitor
fn migrate_deprecated_watchdog_keys(parsed: &mut Value) {
    let watchdog = match parsed.get_mut("watchdog") {
        Some(v) if v.is_mapping() => v,
        _ => return,
    };

    let has_tier0 = watchdog.get("tier0Enabled").is_some();
    let has_tier1 = watchdog.get("tier1Enabled").is_some();

    // Only migrate if old-style: tier1Enabled present but tier0Enabled absent
    if has_tier1 && !has_tier0 {
        if let Value::Mapping(ref mut wd) = watchdog {
            // tier1Enabled → tier0Enabled (mechanical daemon)
            if let Some(v) = wd.remove("tier1Enabled") {
                eprintln!(
                    "[grove] DEPRECATED: watchdog.tier1Enabled → use watchdog.tier0Enabled"
                );
                wd.insert(Value::String("tier0Enabled".to_string()), v);
            }
            // tier1IntervalMs → tier0IntervalMs
            if let Some(v) = wd.remove("tier1IntervalMs") {
                eprintln!(
                    "[grove] DEPRECATED: watchdog.tier1IntervalMs → use watchdog.tier0IntervalMs"
                );
                wd.insert(Value::String("tier0IntervalMs".to_string()), v);
            }
            // tier2Enabled → tier1Enabled (AI triage)
            if let Some(v) = wd.remove("tier2Enabled") {
                eprintln!(
                    "[grove] DEPRECATED: watchdog.tier2Enabled → use watchdog.tier1Enabled"
                );
                wd.insert(Value::String("tier1Enabled".to_string()), v);
            }
        }
    }
}

/// Migrate deprecated task tracker key names (beads/seeds → taskTracker).
fn migrate_deprecated_task_tracker_keys(parsed: &mut Value) {
    if parsed.get("taskTracker").is_some() {
        return; // already migrated
    }

    let mapping = match parsed {
        Value::Mapping(ref mut m) => m,
        _ => return,
    };

    if let Some(beads) = mapping.remove("beads") {
        let enabled = beads
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        eprintln!(
            "[grove] DEPRECATED: beads: → use taskTracker: {{ backend: beads, enabled: true }}"
        );
        let mut tt = serde_yaml::Mapping::new();
        tt.insert(
            Value::String("backend".to_string()),
            Value::String("beads".to_string()),
        );
        tt.insert(Value::String("enabled".to_string()), Value::Bool(enabled));
        mapping.insert(
            Value::String("taskTracker".to_string()),
            Value::Mapping(tt),
        );
    } else if let Some(seeds) = mapping.remove("seeds") {
        let enabled = seeds
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        eprintln!(
            "[grove] DEPRECATED: seeds: → use taskTracker: {{ backend: seeds, enabled: true }}"
        );
        let mut tt = serde_yaml::Mapping::new();
        tt.insert(
            Value::String("backend".to_string()),
            Value::String("seeds".to_string()),
        );
        tt.insert(Value::String("enabled".to_string()), Value::Bool(enabled));
        mapping.insert(
            Value::String("taskTracker".to_string()),
            Value::Mapping(tt),
        );
    }
}

/// Validate a fully-merged config.
fn validate_config(config: &OverstoryConfig) -> Result<(), ValidationError> {
    if config.project.root.is_empty() {
        return Err(
            ValidationError::new("project.root is required and must be a non-empty string")
                .with_field("project.root"),
        );
    }

    if config.project.canonical_branch.is_empty() {
        return Err(ValidationError::new(
            "project.canonicalBranch is required and must be a non-empty string",
        )
        .with_field("project.canonicalBranch"));
    }

    if config.agents.max_concurrent < 1 {
        return Err(
            ValidationError::new("agents.maxConcurrent must be a positive integer")
                .with_field("agents.maxConcurrent")
                .with_value(config.agents.max_concurrent),
        );
    }

    if config.agents.stagger_delay_ms > i64::MAX as u64 {
        return Err(
            ValidationError::new("agents.staggerDelayMs must be non-negative")
                .with_field("agents.staggerDelayMs"),
        );
    }

    if config.watchdog.tier0_enabled && config.watchdog.tier0_interval_ms == 0 {
        return Err(ValidationError::new(
            "watchdog.tier0IntervalMs must be positive when tier0 is enabled",
        )
        .with_field("watchdog.tier0IntervalMs")
        .with_value(config.watchdog.tier0_interval_ms));
    }

    if config.watchdog.nudge_interval_ms == 0 {
        return Err(
            ValidationError::new("watchdog.nudgeIntervalMs must be positive")
                .with_field("watchdog.nudgeIntervalMs"),
        );
    }

    if config.watchdog.stale_threshold_ms == 0 {
        return Err(
            ValidationError::new("watchdog.staleThresholdMs must be positive")
                .with_field("watchdog.staleThresholdMs"),
        );
    }

    if config.watchdog.zombie_threshold_ms <= config.watchdog.stale_threshold_ms {
        return Err(ValidationError::new(
            "watchdog.zombieThresholdMs must be greater than staleThresholdMs",
        )
        .with_field("watchdog.zombieThresholdMs")
        .with_value(config.watchdog.zombie_threshold_ms));
    }

    let valid_formats = ["markdown", "xml", "json"];
    if !valid_formats.contains(&config.mulch.prime_format.as_str()) {
        return Err(ValidationError::new(format!(
            "mulch.primeFormat must be one of: {}",
            valid_formats.join(", ")
        ))
        .with_field("mulch.primeFormat")
        .with_value(&config.mulch.prime_format));
    }

    // taskTracker.backend validation
    match config.task_tracker.backend {
        TaskTrackerBackend::Auto | TaskTrackerBackend::Seeds | TaskTrackerBackend::Beads => {}
    }

    // providers validation
    let valid_provider_types = ["native", "gateway"];
    for (name, provider) in &config.providers {
        if !valid_provider_types.contains(&provider.provider_type.as_str()) {
            return Err(ValidationError::new(format!(
                "providers.{name}.type must be one of: {}",
                valid_provider_types.join(", ")
            ))
            .with_field(format!("providers.{name}.type"))
            .with_value(&provider.provider_type));
        }
        if provider.provider_type == "gateway" {
            if provider.base_url.as_deref().unwrap_or("").is_empty() {
                return Err(ValidationError::new(format!(
                    "providers.{name}.baseUrl is required for gateway providers"
                ))
                .with_field(format!("providers.{name}.baseUrl")));
            }
            if provider.auth_token_env.as_deref().unwrap_or("").is_empty() {
                return Err(ValidationError::new(format!(
                    "providers.{name}.authTokenEnv is required for gateway providers"
                ))
                .with_field(format!("providers.{name}.authTokenEnv")));
            }
        }
    }

    // qualityGates validation
    if let Some(gates) = &config.project.quality_gates {
        for (i, gate) in gates.iter().enumerate() {
            if gate.name.is_empty() {
                return Err(ValidationError::new(format!(
                    "project.qualityGates[{i}].name must be a non-empty string"
                ))
                .with_field(format!("project.qualityGates[{i}].name")));
            }
            if gate.command.is_empty() {
                return Err(ValidationError::new(format!(
                    "project.qualityGates[{i}].command must be a non-empty string"
                ))
                .with_field(format!("project.qualityGates[{i}].command")));
            }
            if gate.description.is_empty() {
                return Err(ValidationError::new(format!(
                    "project.qualityGates[{i}].description must be a non-empty string"
                ))
                .with_field(format!("project.qualityGates[{i}].description")));
            }
        }
    }

    // coordinator.exitTriggers validated by type system (all bool fields)

    // runtime.pi validation
    if let Some(runtime) = &config.runtime {
        if let Some(pi) = &runtime.pi {
            if pi.provider.is_empty() {
                return Err(
                    ValidationError::new("runtime.pi.provider must be a non-empty string")
                        .with_field("runtime.pi.provider"),
                );
            }
            for (alias, qualified) in &pi.model_map {
                if qualified.is_empty() {
                    return Err(ValidationError::new(format!(
                        "runtime.pi.modelMap.{alias} must be a non-empty string"
                    ))
                    .with_field(format!("runtime.pi.modelMap.{alias}")));
                }
            }
        }

        // runtime.shellInitDelayMs: warn if > 30s (non-fatal)
        if let Some(delay) = runtime.shell_init_delay_ms {
            if delay > 30_000 {
                eprintln!(
                    "[grove] WARNING: runtime.shellInitDelayMs is {delay}ms (>30s). \
                     This adds delay before every agent spawn. Consider a lower value."
                );
            }
        }

        // runtime.capabilities: validate each value is non-empty
        if let Some(caps) = &runtime.capabilities {
            for (cap, runtime_name) in caps {
                if runtime_name.is_empty() {
                    return Err(ValidationError::new(format!(
                        "runtime.capabilities.{cap} must be a non-empty string"
                    ))
                    .with_field(format!("runtime.capabilities.{cap}")));
                }
            }
        }
    }

    // models validation
    let valid_aliases = ["sonnet", "opus", "haiku"];
    let tool_heavy_roles = ["builder", "scout"];
    let default_runtime = config
        .runtime
        .as_ref()
        .map(|r| r.default.as_str())
        .unwrap_or("claude");
    let allow_bare_model_refs = default_runtime == "codex";

    for (role, model) in &config.models {
        if model.contains('/') {
            let provider_name = model.split('/').next().unwrap_or("");
            if provider_name.is_empty() || !config.providers.contains_key(provider_name) {
                return Err(ValidationError::new(format!(
                    "models.{role} references unknown provider '{provider_name}'. \
                     Add it to the providers section first."
                ))
                .with_field(format!("models.{role}"))
                .with_value(model));
            }
            if tool_heavy_roles.contains(&role.as_str()) {
                eprintln!(
                    "[grove] WARNING: models.{role} uses non-Anthropic model '{model}'. \
                     Tool-use compatibility cannot be verified at config time."
                );
            }
        } else if !valid_aliases.contains(&model.as_str()) {
            if allow_bare_model_refs {
                if tool_heavy_roles.contains(&role.as_str()) {
                    eprintln!(
                        "[grove] WARNING: models.{role} uses non-Anthropic model '{model}'. \
                         Tool-use compatibility cannot be verified at config time."
                    );
                }
            } else {
                return Err(ValidationError::new(format!(
                    "models.{role} must be a valid alias ({}) or a provider-prefixed ref \
                     (e.g., openrouter/openai/gpt-4)",
                    valid_aliases.join(", ")
                ))
                .with_field(format!("models.{role}"))
                .with_value(model));
            }
        }
    }

    Ok(())
}

/// Deserialize a YAML Value into OverstoryConfig, merging with defaults.
fn value_to_config(
    default: &OverstoryConfig,
    value: Value,
) -> Result<OverstoryConfig, GroveError> {
    let default_value = serde_yaml::to_value(default)
        .map_err(|e| ConfigError::new(format!("Failed to serialize defaults: {e}")))?;
    let merged = deep_merge(default_value, value);
    let config: OverstoryConfig = serde_yaml::from_value(merged)
        .map_err(|e| ConfigError::new(format!("Failed to deserialize config: {e}")))?;
    Ok(config)
}

/// Read and parse a YAML config file. Returns None if the file does not exist.
fn read_yaml_file(path: &Path) -> Result<Option<Value>, GroveError> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path).map_err(|_e| {
        ConfigError::new(format!("Failed to read config file: {}", path.display()))
            .with_path(path.display().to_string())
    })?;
    let value: Value = serde_yaml::from_str(&text).map_err(|e| {
        ConfigError::new(format!(
            "Failed to parse YAML in config file: {} — {e}",
            path.display()
        ))
        .with_path(path.display().to_string())
    })?;
    Ok(Some(value))
}

/// Load the overstory configuration for a project.
///
/// Reads `.overstory/config.yaml` from the project root, merges with defaults,
/// merges `config.local.yaml` if present, and validates the result.
///
/// # Arguments
/// * `project_root` — Absolute path to the target project root (or worktree)
/// * `override_path` — Optional explicit project root override (from `--project` flag)
pub fn load_config(
    project_root: &Path,
    override_path: Option<&Path>,
) -> Result<OverstoryConfig, GroveError> {
    let resolved_root = resolve_project_root(project_root, override_path)?;
    let config_path = resolved_root
        .join(OVERSTORY_DIR)
        .join(CONFIG_FILENAME);
    let local_config_path = resolved_root
        .join(OVERSTORY_DIR)
        .join(CONFIG_LOCAL_FILENAME);

    // Build defaults with the resolved root and project name injected.
    let mut defaults = OverstoryConfig::default();
    defaults.project.root = resolved_root.to_string_lossy().into_owned();
    defaults.project.name = resolved_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".to_string());

    // Read main config file
    let main_value = read_yaml_file(&config_path)?;

    let mut config = match main_value {
        None => {
            // No config file — use defaults, then check for local overrides.
            defaults.clone()
        }
        Some(mut parsed) => {
            migrate_deprecated_watchdog_keys(&mut parsed);
            migrate_deprecated_task_tracker_keys(&mut parsed);
            value_to_config(&defaults, parsed)?
        }
    };

    // Merge config.local.yaml if present
    if let Some(mut local_parsed) = read_yaml_file(&local_config_path)? {
        migrate_deprecated_watchdog_keys(&mut local_parsed);
        migrate_deprecated_task_tracker_keys(&mut local_parsed);
        // Re-serialize current config as Value and merge local on top
        let current_value = serde_yaml::to_value(&config)
            .map_err(|e| ConfigError::new(format!("Failed to serialize config for local merge: {e}")))?;
        let merged = deep_merge(current_value, local_parsed);
        config = serde_yaml::from_value(merged)
            .map_err(|e| ConfigError::new(format!("Failed to deserialize after local merge: {e}")))?;
    }

    // Always set project.root to the resolved root (overrides whatever was in YAML)
    config.project.root = resolved_root.to_string_lossy().into_owned();

    // Merge verification defaults when section is present but partial
    if let Some(ref verification) = config.project.verification {
        let defaults_v = default_verification_config();
        config.project.verification = Some(crate::types::VerificationConfig {
            dev_server_command: verification
                .dev_server_command
                .clone()
                .or(defaults_v.dev_server_command),
            base_url: verification.base_url.clone().or(defaults_v.base_url),
            port: verification.port.or(defaults_v.port),
            routes: verification.routes.clone().or(defaults_v.routes),
            viewports: verification.viewports.clone().or(defaults_v.viewports),
        });
    }

    validate_config(&config)?;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_file(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    fn minimal_config_yaml(root: &str) -> String {
        format!(
            r#"
project:
  name: test-project
  root: {root}
  canonicalBranch: main
"#
        )
    }

    #[test]
    fn load_config_with_no_file_uses_defaults() {
        let dir = TempDir::new().unwrap();
        // No config.yaml — resolve_project_root will return dir path
        // but validation will fail because project.root is set from directory
        // We need to simulate having the .overstory dir
        std::fs::create_dir_all(dir.path().join(".overstory")).unwrap();

        let result = load_config(dir.path(), None);
        // Should succeed with defaults (project.root is set to resolved_root)
        assert!(result.is_ok(), "Expected Ok, got: {result:?}");
        let config = result.unwrap();
        assert_eq!(config.project.canonical_branch, "main");
        assert_eq!(config.agents.max_concurrent, 25);
    }

    #[test]
    fn load_config_parses_yaml() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            ".overstory/config.yaml",
            &format!(
                r#"
project:
  name: my-project
  root: {}
  canonicalBranch: develop
agents:
  maxConcurrent: 10
  staggerDelayMs: 1000
  maxDepth: 3
  maxSessionsPerRun: 0
  maxAgentsPerLead: 5
  manifestPath: .overstory/agent-manifest.json
  baseDir: .overstory/agent-defs
"#,
                dir.path().display()
            ),
        );

        let config = load_config(dir.path(), None).unwrap();
        assert_eq!(config.project.canonical_branch, "develop");
        assert_eq!(config.project.name, "my-project");
        assert_eq!(config.agents.max_concurrent, 10);
        assert_eq!(config.agents.stagger_delay_ms, 1_000);
        assert_eq!(config.agents.max_depth, 3);
    }

    #[test]
    fn load_config_merges_local_yaml() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().display().to_string();
        write_file(
            dir.path(),
            ".overstory/config.yaml",
            &format!(
                r#"
project:
  name: my-project
  root: {root}
  canonicalBranch: main
agents:
  maxConcurrent: 25
  staggerDelayMs: 2000
  maxDepth: 2
  maxSessionsPerRun: 0
  maxAgentsPerLead: 5
  manifestPath: .overstory/agent-manifest.json
  baseDir: .overstory/agent-defs
"#
            ),
        );
        write_file(
            dir.path(),
            ".overstory/config.local.yaml",
            "agents:\n  maxConcurrent: 3\n",
        );

        let config = load_config(dir.path(), None).unwrap();
        assert_eq!(config.agents.max_concurrent, 3);
        // Other fields still from main config
        assert_eq!(config.agents.stagger_delay_ms, 2_000);
    }

    #[test]
    fn load_config_override_path() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().display().to_string();
        write_file(
            dir.path(),
            ".overstory/config.yaml",
            &format!(
                r#"
project:
  name: override-project
  root: {root}
  canonicalBranch: main
"#
            ),
        );

        // Pass override_path = dir.path()
        let config = load_config(Path::new("/tmp"), Some(dir.path())).unwrap();
        assert_eq!(config.project.name, "override-project");
    }

    #[test]
    fn validate_config_rejects_zero_max_concurrent() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().display().to_string();
        write_file(
            dir.path(),
            ".overstory/config.yaml",
            &format!(
                r#"
project:
  name: bad
  root: {root}
  canonicalBranch: main
agents:
  maxConcurrent: 0
  staggerDelayMs: 2000
  maxDepth: 2
  maxSessionsPerRun: 0
  maxAgentsPerLead: 5
  manifestPath: .overstory/agent-manifest.json
  baseDir: .overstory/agent-defs
"#
            ),
        );

        let result = load_config(dir.path(), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("maxConcurrent"),
            "Expected maxConcurrent error, got: {err}"
        );
    }

    #[test]
    fn validate_config_rejects_bad_prime_format() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().display().to_string();
        write_file(
            dir.path(),
            ".overstory/config.yaml",
            &format!(
                r#"
project:
  name: bad
  root: {root}
  canonicalBranch: main
mulch:
  enabled: true
  domains: []
  primeFormat: invalid
"#
            ),
        );

        let result = load_config(dir.path(), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("primeFormat"),
            "Expected primeFormat error, got: {err}"
        );
    }

    #[test]
    fn migrate_watchdog_tier_keys() {
        let mut v: Value = serde_yaml::from_str(
            r#"
watchdog:
  tier1Enabled: true
  tier1IntervalMs: 15000
  tier2Enabled: true
"#,
        )
        .unwrap();

        migrate_deprecated_watchdog_keys(&mut v);

        let wd = v.get("watchdog").unwrap();
        assert!(wd.get("tier0Enabled").is_some());
        assert_eq!(wd.get("tier0Enabled").unwrap().as_bool(), Some(true));
        assert!(wd.get("tier0IntervalMs").is_some());
        assert_eq!(
            wd.get("tier0IntervalMs").unwrap().as_u64(),
            Some(15_000)
        );
        assert!(wd.get("tier1Enabled").is_some()); // was tier2Enabled
        assert!(wd.get("tier2Enabled").is_none()); // removed
    }

    #[test]
    fn migrate_task_tracker_from_beads() {
        let mut v: Value = serde_yaml::from_str(
            r#"
beads:
  enabled: true
"#,
        )
        .unwrap();

        migrate_deprecated_task_tracker_keys(&mut v);

        assert!(v.get("taskTracker").is_some());
        assert!(v.get("beads").is_none());
        let tt = v.get("taskTracker").unwrap();
        assert_eq!(tt.get("backend").unwrap().as_str(), Some("beads"));
    }

    #[test]
    fn migrate_task_tracker_from_seeds() {
        let mut v: Value = serde_yaml::from_str(
            r#"
seeds:
  enabled: false
"#,
        )
        .unwrap();

        migrate_deprecated_task_tracker_keys(&mut v);

        let tt = v.get("taskTracker").unwrap();
        assert_eq!(tt.get("backend").unwrap().as_str(), Some("seeds"));
        assert_eq!(tt.get("enabled").unwrap().as_bool(), Some(false));
    }

    #[test]
    fn deep_merge_prefers_source() {
        let target: Value = serde_yaml::from_str("a: 1\nb: 2").unwrap();
        let source: Value = serde_yaml::from_str("b: 99\nc: 3").unwrap();
        let merged = deep_merge(target, source);
        assert_eq!(merged.get("a").unwrap().as_u64(), Some(1));
        assert_eq!(merged.get("b").unwrap().as_u64(), Some(99));
        assert_eq!(merged.get("c").unwrap().as_u64(), Some(3));
    }

    #[test]
    fn deep_merge_arrays_replaced() {
        let target: Value = serde_yaml::from_str("items:\n  - a\n  - b").unwrap();
        let source: Value = serde_yaml::from_str("items:\n  - x").unwrap();
        let merged = deep_merge(target, source);
        let items = merged
            .get("items")
            .unwrap()
            .as_sequence()
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].as_str(), Some("x"));
    }

    #[test]
    fn resolve_project_root_uses_override() {
        let dir = TempDir::new().unwrap();
        let result = resolve_project_root(Path::new("/tmp"), Some(dir.path())).unwrap();
        assert_eq!(result, dir.path());
    }

    #[test]
    fn load_config_validates_gateway_provider() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().display().to_string();
        write_file(
            dir.path(),
            ".overstory/config.yaml",
            &format!(
                r#"
project:
  name: gw-test
  root: {root}
  canonicalBranch: main
providers:
  anthropic:
    type: native
  mygateway:
    type: gateway
"#
            ),
        );

        let result = load_config(dir.path(), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("baseUrl"),
            "Expected baseUrl error, got: {err}"
        );
    }
}
