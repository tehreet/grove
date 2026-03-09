#![allow(dead_code)]

use serde::Serialize;
use serde_json::json;

/// Write a JSON success envelope to stdout.
/// Format: { "success": true, "command": "<name>", ...data }
pub fn json_output<T: Serialize>(command: &str, data: &T) -> String {
    let mut value = serde_json::to_value(data).unwrap_or_else(|_| json!({}));
    if let Some(obj) = value.as_object_mut() {
        obj.insert("success".into(), json!(true));
        obj.insert("command".into(), json!(command));
    }
    serde_json::to_string(&value).unwrap_or_default()
}

/// Write a JSON error envelope to stdout.
/// Format: { "success": false, "command": "<name>", "error": "<message>" }
pub fn json_error(command: &str, error: &str) -> String {
    serde_json::to_string(&json!({
        "success": false,
        "command": command,
        "error": error,
    }))
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[derive(Serialize)]
    struct TestData {
        count: u32,
        name: String,
    }

    #[test]
    fn json_output_includes_success_and_command() {
        let data = TestData {
            count: 3,
            name: "foo".into(),
        };
        let out = json_output("sessions list", &data);
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["success"], true);
        assert_eq!(v["command"], "sessions list");
    }

    #[test]
    fn json_output_spreads_data_fields() {
        let data = TestData {
            count: 7,
            name: "bar".into(),
        };
        let out = json_output("test", &data);
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["count"], 7);
        assert_eq!(v["name"], "bar");
    }

    #[test]
    fn json_error_includes_success_false_and_error() {
        let out = json_error("sessions list", "database unavailable");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["success"], false);
        assert_eq!(v["command"], "sessions list");
        assert_eq!(v["error"], "database unavailable");
    }

    #[test]
    fn json_error_no_data_fields() {
        let out = json_error("mail send", "agent not found");
        let v: Value = serde_json::from_str(&out).unwrap();
        // Only these three keys
        let obj = v.as_object().unwrap();
        assert_eq!(obj.len(), 3);
        assert!(obj.contains_key("success"));
        assert!(obj.contains_key("command"));
        assert!(obj.contains_key("error"));
    }

    #[test]
    fn json_output_roundtrip_parse() {
        #[derive(Serialize, serde::Deserialize, PartialEq, Debug)]
        struct Payload {
            items: Vec<String>,
        }
        let data = Payload {
            items: vec!["a".into(), "b".into()],
        };
        let out = json_output("list", &data);
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["success"], true);
        assert_eq!(v["items"][0], "a");
        assert_eq!(v["items"][1], "b");
    }

    #[test]
    fn json_error_output_is_valid_json() {
        let out = json_error("merge", "conflict detected");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert!(v.is_object());
    }
}
