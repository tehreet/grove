use thiserror::Error;

#[derive(Debug, Error)]
pub enum GroveError {
    #[error("config error: {message}")]
    Config {
        message: String,
        config_path: Option<String>,
        field: Option<String>,
    },

    #[error("agent error: {message}")]
    Agent {
        message: String,
        agent_name: Option<String>,
        capability: Option<String>,
    },

    #[error("hierarchy violation: {message}")]
    Hierarchy {
        message: String,
        agent_name: Option<String>,
        requested_capability: Option<String>,
    },

    #[error("worktree error: {message}")]
    Worktree {
        message: String,
        worktree_path: Option<String>,
        branch_name: Option<String>,
    },

    #[error("mail error: {message}")]
    Mail {
        message: String,
        agent_name: Option<String>,
        message_id: Option<String>,
    },

    #[error("merge error: {message}")]
    Merge {
        message: String,
        branch_name: Option<String>,
        conflict_files: Vec<String>,
    },

    #[error("validation error: {message}")]
    Validation {
        message: String,
        field: Option<String>,
    },

    #[error("group error: {message}")]
    Group {
        message: String,
        group_id: Option<String>,
    },

    #[error("lifecycle error: {message}")]
    Lifecycle {
        message: String,
        agent_name: Option<String>,
        session_id: Option<String>,
    },

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

/// Alias for Result with GroveError.
pub type Result<T> = std::result::Result<T, GroveError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_error_display() {
        let err = GroveError::Config {
            message: "missing field".into(),
            config_path: Some("/etc/grove.yaml".into()),
            field: Some("agents.maxConcurrent".into()),
        };
        assert_eq!(err.to_string(), "config error: missing field");
    }

    #[test]
    fn agent_error_display() {
        let err = GroveError::Agent {
            message: "agent not found".into(),
            agent_name: Some("builder-1".into()),
            capability: None,
        };
        assert_eq!(err.to_string(), "agent error: agent not found");
    }

    #[test]
    fn hierarchy_error_display() {
        let err = GroveError::Hierarchy {
            message: "depth limit exceeded".into(),
            agent_name: Some("builder-2".into()),
            requested_capability: Some("coordinator".into()),
        };
        assert_eq!(err.to_string(), "hierarchy violation: depth limit exceeded");
    }

    #[test]
    fn worktree_error_display() {
        let err = GroveError::Worktree {
            message: "branch conflict".into(),
            worktree_path: Some("/tmp/wt".into()),
            branch_name: Some("feat/x".into()),
        };
        assert_eq!(err.to_string(), "worktree error: branch conflict");
    }

    #[test]
    fn mail_error_display() {
        let err = GroveError::Mail {
            message: "delivery failed".into(),
            agent_name: Some("builder-1".into()),
            message_id: Some("msg-abc".into()),
        };
        assert_eq!(err.to_string(), "mail error: delivery failed");
    }

    #[test]
    fn merge_error_display() {
        let err = GroveError::Merge {
            message: "unresolvable conflict".into(),
            branch_name: Some("feat/y".into()),
            conflict_files: vec!["src/main.rs".into()],
        };
        assert_eq!(err.to_string(), "merge error: unresolvable conflict");
    }

    #[test]
    fn validation_error_display() {
        let err = GroveError::Validation {
            message: "invalid task ID".into(),
            field: Some("taskId".into()),
        };
        assert_eq!(err.to_string(), "validation error: invalid task ID");
    }

    #[test]
    fn group_error_display() {
        let err = GroveError::Group {
            message: "group not found".into(),
            group_id: Some("group-abc".into()),
        };
        assert_eq!(err.to_string(), "group error: group not found");
    }

    #[test]
    fn lifecycle_error_display() {
        let err = GroveError::Lifecycle {
            message: "checkpoint save failed".into(),
            agent_name: Some("builder-1".into()),
            session_id: Some("sess-1".into()),
        };
        assert_eq!(err.to_string(), "lifecycle error: checkpoint save failed");
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let grove_err: GroveError = io_err.into();
        assert!(matches!(grove_err, GroveError::Io(_)));
        assert!(grove_err.to_string().starts_with("io error:"));
    }

    #[test]
    fn from_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json {{{").unwrap_err();
        let grove_err: GroveError = json_err.into();
        assert!(matches!(grove_err, GroveError::Json(_)));
        assert!(grove_err.to_string().starts_with("json error:"));
    }

    #[test]
    fn from_yaml_error() {
        let yaml_err = serde_yaml::from_str::<serde_json::Value>("key: : :\n  bad").unwrap_err();
        let grove_err: GroveError = yaml_err.into();
        assert!(matches!(grove_err, GroveError::Yaml(_)));
        assert!(grove_err.to_string().starts_with("yaml error:"));
    }

    #[test]
    fn from_rusqlite_error() {
        let sql_err = rusqlite::Error::InvalidParameterCount(0, 1);
        let grove_err: GroveError = sql_err.into();
        assert!(matches!(grove_err, GroveError::Database(_)));
        assert!(grove_err.to_string().starts_with("database error:"));
    }

    #[test]
    fn result_alias_ok() {
        let ok: Result<i32> = Ok(42);
        assert_eq!(ok.unwrap(), 42);
    }

    #[test]
    fn result_alias_err() {
        let err: Result<i32> = Err(GroveError::Validation {
            message: "bad".into(),
            field: None,
        });
        assert!(err.is_err());
    }
}
