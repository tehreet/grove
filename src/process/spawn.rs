#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command as TokioCommand};

use crate::errors::{GroveError, Result};

#[allow(dead_code)]
pub struct ManagedProcess {
    pub pid: u32,
    pub child: Child,
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
}

pub async fn spawn_headless(
    binary: &str,
    args: &[String],
    cwd: &Path,
    env: HashMap<String, String>,
) -> Result<ManagedProcess> {
    let mut cmd = TokioCommand::new(binary);
    cmd.args(args)
        .current_dir(cwd)
        .envs(&env)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| GroveError::Agent {
        message: format!("failed to spawn '{}': {}", binary, e),
        agent_name: None,
        capability: None,
    })?;

    let pid = child.id().unwrap_or(0);
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    Ok(ManagedProcess {
        pid,
        child,
        stdin,
        stdout,
        stderr,
    })
}

pub fn is_process_alive(pid: u32) -> bool {
    std::fs::metadata(format!("/proc/{pid}")).is_ok()
}

pub fn kill_process(pid: u32, force: bool) -> Result<()> {
    let signal = if force { "-9" } else { "-15" };
    let status = std::process::Command::new("kill")
        .args([signal, &pid.to_string()])
        .status()
        .map_err(|e| GroveError::Agent {
            message: format!("failed to run kill command: {}", e),
            agent_name: None,
            capability: None,
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(GroveError::Agent {
            message: format!("kill({}, {}) failed with status: {}", pid, signal, status),
            agent_name: None,
            capability: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_is_process_alive_self() {
        let pid = std::process::id();
        assert!(is_process_alive(pid));
    }

    #[test]
    fn test_is_process_alive_nonexistent() {
        assert!(!is_process_alive(999_999_999));
    }

    #[tokio::test]
    async fn test_spawn_headless_success() {
        let proc = spawn_headless(
            "echo",
            &["hello".to_string()],
            &PathBuf::from("/tmp"),
            HashMap::new(),
        )
        .await
        .expect("should spawn echo");
        assert!(proc.pid > 0);
    }

    #[tokio::test]
    async fn test_spawn_headless_bad_binary() {
        let result = spawn_headless(
            "this-binary-definitely-does-not-exist-grove",
            &[],
            &PathBuf::from("/tmp"),
            HashMap::new(),
        )
        .await;
        assert!(result.is_err());
        assert!(matches!(result.err().unwrap(), GroveError::Agent { .. }));
    }
}
