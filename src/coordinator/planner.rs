//! LLM-based task decomposition for the coordinator.
//!
//! When a `dispatch` mail arrives with a task description, the coordinator
//! calls the Claude API (one-shot via reqwest) to decompose it into subtasks.
//! All other coordinator decisions are deterministic — no LLM calls needed.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    pub title: String,
    pub description: String,
    pub capability: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompositionResult {
    pub subtasks: Vec<Subtask>,
    pub reasoning: String,
}

// ---------------------------------------------------------------------------
// Decomposition request/response shapes for Claude API
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ClaudeMessage>,
}

#[derive(Debug, Serialize)]
struct ClaudeMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    content: Vec<ClaudeContent>,
}

#[derive(Debug, Deserialize)]
struct ClaudeContent {
    text: Option<String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Decompose a task description into subtasks via the Claude API.
///
/// Returns a `DecompositionResult` with the list of subtasks and the
/// model's reasoning. Returns an error if the API key is missing or
/// the request fails.
pub async fn decompose_task(
    task_description: &str,
    api_key: &str,
) -> Result<DecompositionResult, String> {
    if api_key.is_empty() {
        return Err("ANTHROPIC_API_KEY is not set".to_string());
    }

    let prompt = format!(
        r#"You are a software project coordinator. Decompose the following task into concrete subtasks for AI coding agents.

Task: {task_description}

Respond with JSON in exactly this format:
{{
  "reasoning": "Brief explanation of the decomposition strategy",
  "subtasks": [
    {{
      "title": "Short title",
      "description": "What the agent should implement",
      "capability": "builder",
      "files": ["src/foo.rs", "src/bar.rs"]
    }}
  ]
}}

Use capability values: builder, lead, reviewer, scout, merger.
Keep subtasks focused — one concern per subtask."#
    );

    let client = reqwest::Client::new();
    let request_body = ClaudeRequest {
        model: "claude-sonnet-4-6".to_string(),
        max_tokens: 2048,
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: prompt,
        }],
    };

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("API request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API error {status}: {body}"));
    }

    let claude_resp: ClaudeResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse API response: {e}"))?;

    let text = claude_resp
        .content
        .into_iter()
        .find_map(|c| c.text)
        .unwrap_or_default();

    // Extract JSON from the response
    let json_start = text.find('{').ok_or("No JSON in API response")?;
    let json_end = text.rfind('}').ok_or("No JSON in API response")? + 1;
    let json_str = &text[json_start..json_end];

    serde_json::from_str::<DecompositionResult>(json_str)
        .map_err(|e| format!("Failed to parse decomposition JSON: {e}"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subtask_serializes() {
        let t = Subtask {
            title: "Implement foo".to_string(),
            description: "Build the foo module".to_string(),
            capability: "builder".to_string(),
            files: vec!["src/foo.rs".to_string()],
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("title"));
        assert!(json.contains("builder"));
    }

    #[test]
    fn test_decomposition_result_roundtrip() {
        let result = DecompositionResult {
            subtasks: vec![Subtask {
                title: "foo".to_string(),
                description: "bar".to_string(),
                capability: "builder".to_string(),
                files: vec![],
            }],
            reasoning: "test".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: DecompositionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.subtasks.len(), 1);
        assert_eq!(parsed.reasoning, "test");
    }

    #[tokio::test]
    async fn test_decompose_task_no_api_key() {
        let result = decompose_task("build something", "").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ANTHROPIC_API_KEY"));
    }
}
