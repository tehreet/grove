//! `grove spec write` — write a task specification file.
//!
//! Writes a spec markdown file to `.overstory/specs/<task-id>.md`.
//! Scouts use this to persist spec documents as files rather than
//! sending entire specs via mail messages.

use std::fs;
use std::path::Path;

use crate::config::resolve_project_root;
use crate::json::json_output;
use crate::logging::print_success;

// ---------------------------------------------------------------------------
// Core logic (pub for testing)
// ---------------------------------------------------------------------------

/// Write a spec file to `.overstory/specs/<task-id>.md`.
///
/// Returns the absolute path to the written file.
pub fn write_spec(
    project_root: &Path,
    task_id: &str,
    body: &str,
    agent: Option<&str>,
) -> Result<String, String> {
    let specs_dir = project_root.join(".overstory").join("specs");
    fs::create_dir_all(&specs_dir).map_err(|e| format!("Failed to create specs directory: {e}"))?;

    let mut content = String::new();
    if let Some(agent_name) = agent {
        content.push_str(&format!("<!-- written-by: {agent_name} -->\n"));
    }
    content.push_str(body);
    if !content.ends_with('\n') {
        content.push('\n');
    }

    let spec_path = specs_dir.join(format!("{task_id}.md"));
    fs::write(&spec_path, &content).map_err(|e| format!("Failed to write spec file: {e}"))?;

    Ok(spec_path.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// Output type
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SpecWriteOutput {
    task_id: String,
    path: String,
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

/// `grove spec write <task-id> --body <content> [--agent <name>] [--file <path>]`
pub fn execute_write(
    task_id: &str,
    body: Option<String>,
    file: Option<&Path>,
    agent: Option<String>,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    if task_id.trim().is_empty() {
        return Err("Task ID is required: grove spec write <task-id> --body <content>".to_string());
    }

    // Resolve body from --body, --file, or stdin
    let content: String = if let Some(b) = body {
        b
    } else if let Some(f) = file {
        fs::read_to_string(f)
            .map_err(|e| format!("Failed to read spec file {}: {e}", f.display()))?
    } else {
        return Err("Spec body is required: use --body <content> or --file <path>".to_string());
    };

    if content.trim().is_empty() {
        return Err("Spec body cannot be empty".to_string());
    }

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let project_root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;

    let spec_path = write_spec(&project_root, task_id, &content, agent.as_deref())?;

    if json {
        let output = SpecWriteOutput {
            task_id: task_id.to_string(),
            path: spec_path,
        };
        println!("{}", json_output("spec-write", &output));
    } else {
        print_success("Spec written", Some(task_id));
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

    fn make_ov_dir(dir: &std::path::Path) {
        fs::create_dir_all(dir.join(".overstory")).unwrap();
    }

    #[test]
    fn test_write_spec_creates_file() {
        let dir = TempDir::new().unwrap();
        make_ov_dir(dir.path());
        let path = write_spec(dir.path(), "grove-001", "# My Spec\n\nDo the thing.", None).unwrap();
        assert!(std::path::Path::new(&path).exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# My Spec"));
        assert!(content.ends_with('\n'));
    }

    #[test]
    fn test_write_spec_adds_agent_attribution() {
        let dir = TempDir::new().unwrap();
        make_ov_dir(dir.path());
        write_spec(dir.path(), "grove-002", "Body text.", Some("lead-agent")).unwrap();
        let spec_path = dir.path().join(".overstory/specs/grove-002.md");
        let content = fs::read_to_string(spec_path).unwrap();
        assert!(content.starts_with("<!-- written-by: lead-agent -->"));
        assert!(content.contains("Body text."));
    }

    #[test]
    fn test_write_spec_ensures_trailing_newline() {
        let dir = TempDir::new().unwrap();
        make_ov_dir(dir.path());
        let path = write_spec(dir.path(), "grove-003", "no newline", None).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.ends_with('\n'));
    }

    #[test]
    fn test_write_spec_creates_specs_dir() {
        let dir = TempDir::new().unwrap();
        // .overstory dir itself created by write_spec
        let path = write_spec(dir.path(), "grove-004", "content", None).unwrap();
        assert!(std::path::Path::new(&path).exists());
    }

    #[test]
    fn test_execute_write_rejects_empty_task_id() {
        let result = execute_write("", Some("body".to_string()), None, None, false, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Task ID is required"));
    }

    #[test]
    fn test_execute_write_rejects_no_body_or_file() {
        let result = execute_write("grove-005", None, None, None, false, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Spec body is required"));
    }

    #[test]
    fn test_execute_write_rejects_empty_body() {
        let result = execute_write(
            "grove-006",
            Some("   ".to_string()),
            None,
            None,
            false,
            None,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cannot be empty"));
    }

    #[test]
    fn test_write_spec_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        make_ov_dir(dir.path());
        write_spec(dir.path(), "grove-007", "first version", None).unwrap();
        write_spec(dir.path(), "grove-007", "second version", None).unwrap();
        let spec_path = dir.path().join(".overstory/specs/grove-007.md");
        let content = fs::read_to_string(spec_path).unwrap();
        assert!(content.contains("second version"));
        assert!(!content.contains("first version"));
    }
}
