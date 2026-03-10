#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::ChildStdout;

use crate::errors::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentOutcome {
    Completed {
        #[serde(rename = "exitCode")]
        exit_code: i32,
    },
    BudgetExceeded {
        #[serde(rename = "costSoFar")]
        cost_so_far: f64,
    },
    Crashed {
        signal: Option<i32>,
    },
}

pub async fn monitor_agent_stdout(
    stdout: ChildStdout,
    _agent_name: &str,
    budget_limit: Option<f64>,
) -> Result<AgentOutcome> {
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();
    let mut cumulative_cost: f64 = 0.0;

    while let Some(line) = lines.next_line().await? {
        // Attempt NDJSON parse; skip non-JSON lines
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
            // Look for cost data in result events
            if val.get("type").and_then(|t| t.as_str()) == Some("result") {
                if let Some(cost) = val.get("cost_usd").and_then(|c| c.as_f64()) {
                    cumulative_cost += cost;
                } else if let Some(cost) = val.pointer("/statistics/cost").and_then(|c| c.as_f64())
                {
                    cumulative_cost += cost;
                }

                if let Some(limit) = budget_limit {
                    if cumulative_cost > limit {
                        return Ok(AgentOutcome::BudgetExceeded {
                            cost_so_far: cumulative_cost,
                        });
                    }
                }
            }
        }
    }

    Ok(AgentOutcome::Completed { exit_code: 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_outcome_serde() {
        let completed = AgentOutcome::Completed { exit_code: 0 };
        let json = serde_json::to_string(&completed).unwrap();
        assert!(json.contains("\"type\":\"completed\""));
        assert!(json.contains("\"exitCode\":0"));
        let back: AgentOutcome = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, AgentOutcome::Completed { exit_code: 0 }));

        let budget = AgentOutcome::BudgetExceeded { cost_so_far: 1.5 };
        let json = serde_json::to_string(&budget).unwrap();
        assert!(json.contains("\"type\":\"budgetExceeded\""));
        assert!(json.contains("\"costSoFar\":1.5"));

        let crashed = AgentOutcome::Crashed { signal: Some(9) };
        let json = serde_json::to_string(&crashed).unwrap();
        assert!(json.contains("\"type\":\"crashed\""));
    }

    #[tokio::test]
    async fn test_monitor_empty_stdout() {
        // Spawn a command that produces no output and exits immediately
        let mut child = tokio::process::Command::new("true")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("should spawn true");
        let stdout = child.stdout.take().unwrap();
        let outcome = monitor_agent_stdout(stdout, "test-agent", None)
            .await
            .unwrap();
        assert!(matches!(outcome, AgentOutcome::Completed { exit_code: 0 }));
    }

    #[tokio::test]
    async fn test_monitor_budget_exceeded() {
        // Spawn echo with a JSON result line where cost_usd=5.0 exceeds budget of 1.0
        let cost_json = r#"{"type":"result","cost_usd":5.0}"#;
        let mut child = tokio::process::Command::new("echo")
            .arg(cost_json)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("spawn echo");
        let stdout = child.stdout.take().unwrap();
        let outcome = monitor_agent_stdout(stdout, "test-agent", Some(1.0))
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            AgentOutcome::BudgetExceeded { cost_so_far } if cost_so_far > 1.0
        ));
    }
}
