use thiserror::Error;

/// Top-level error type for all Grove operations.
#[derive(Debug, Error)]
pub enum GroveError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Agent(#[from] AgentError),
    #[error(transparent)]
    Hierarchy(#[from] HierarchyError),
    #[error(transparent)]
    Worktree(#[from] WorktreeError),
    #[error(transparent)]
    Mail(#[from] MailError),
    #[error(transparent)]
    Merge(#[from] MergeError),
    #[error(transparent)]
    Group(#[from] GroupError),
    #[error(transparent)]
    Lifecycle(#[from] LifecycleError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("{0}")]
    Other(String),
}

/// Raised when config loading or validation fails.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct ConfigError {
    pub message: String,
    pub config_path: Option<String>,
    pub field: Option<String>,
}

impl ConfigError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            config_path: None,
            field: None,
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.config_path = Some(path.into());
        self
    }

    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }
}

/// Raised when input validation fails.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct ValidationError {
    pub message: String,
    pub field: Option<String>,
    pub value: Option<String>,
}

impl ValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            field: None,
            value: None,
        }
    }

    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    pub fn with_value(mut self, value: impl std::fmt::Debug) -> Self {
        self.value = Some(format!("{value:?}"));
        self
    }
}

/// Raised for agent lifecycle issues.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct AgentError {
    pub message: String,
    pub agent_name: Option<String>,
    pub capability: Option<String>,
}

impl AgentError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            agent_name: None,
            capability: None,
        }
    }

    pub fn with_agent(mut self, name: impl Into<String>) -> Self {
        self.agent_name = Some(name.into());
        self
    }

    pub fn with_capability(mut self, cap: impl Into<String>) -> Self {
        self.capability = Some(cap.into());
        self
    }
}

/// Raised when hierarchy constraints are violated.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct HierarchyError {
    pub message: String,
    pub agent_name: Option<String>,
    pub requested_capability: Option<String>,
}

impl HierarchyError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            agent_name: None,
            requested_capability: None,
        }
    }
}

/// Raised when git worktree operations fail.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct WorktreeError {
    pub message: String,
    pub worktree_path: Option<String>,
    pub branch_name: Option<String>,
}

impl WorktreeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            worktree_path: None,
            branch_name: None,
        }
    }
}

/// Raised when mail system operations fail.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct MailError {
    pub message: String,
    pub agent_name: Option<String>,
    pub message_id: Option<String>,
}

impl MailError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            agent_name: None,
            message_id: None,
        }
    }
}

/// Raised when merge or conflict resolution fails.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct MergeError {
    pub message: String,
    pub branch_name: Option<String>,
    pub conflict_files: Vec<String>,
}

impl MergeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            branch_name: None,
            conflict_files: vec![],
        }
    }
}

/// Raised when task group operations fail.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct GroupError {
    pub message: String,
    pub group_id: Option<String>,
}

impl GroupError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            group_id: None,
        }
    }
}

/// Raised when session lifecycle operations fail.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct LifecycleError {
    pub message: String,
    pub agent_name: Option<String>,
    pub session_id: Option<String>,
}

impl LifecycleError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            agent_name: None,
            session_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_error_builder() {
        let err = ConfigError::new("bad config")
            .with_path("/some/path")
            .with_field("project.root");
        assert_eq!(err.message, "bad config");
        assert_eq!(err.config_path.as_deref(), Some("/some/path"));
        assert_eq!(err.field.as_deref(), Some("project.root"));
        assert_eq!(err.to_string(), "bad config");
    }

    #[test]
    fn validation_error_builder() {
        let err = ValidationError::new("must be positive")
            .with_field("agents.maxConcurrent")
            .with_value(-1i32);
        assert_eq!(err.field.as_deref(), Some("agents.maxConcurrent"));
        assert!(err.value.is_some());
    }

    #[test]
    fn grove_error_from_config() {
        let config_err = ConfigError::new("oops");
        let grove_err: GroveError = config_err.into();
        assert!(matches!(grove_err, GroveError::Config(_)));
    }

    #[test]
    fn grove_error_from_validation() {
        let val_err = ValidationError::new("invalid");
        let grove_err: GroveError = val_err.into();
        assert!(matches!(grove_err, GroveError::Validation(_)));
    }
}
