//! The project `.mcp.json` an attached Pi can read.
//!
//! `pi-acp` stores the servers from `session/new` and does not pass them to
//! `pi`. The plugin reads this file from the process's working directory.
//! The port and the token change every launch, and the adapter expands
//! `${AI_BUDDY_MCP_URL}` and `bearerTokenEnv`, so the file names those
//! variables and a second Apply leaves a correct file alone.

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

const FILE: &str = ".mcp.json";

fn entry() -> Value {
    json!({
        "url": "${AI_BUDDY_MCP_URL}",
        "auth": "bearer",
        "bearerTokenEnv": "AI_BUDDY_MCP_TOKEN",
        "lifecycle": "eager",
    })
}

/// Create `<dir>/.mcp.json`, or replace only its `ai-buddy` server, when that
/// object is not already the stable entry. The directory is not created.
pub fn sync_project_file(dir: &Path) -> Result<(), String> {
    let path = dir.join(FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return write(&path, &json!({ "mcpServers": { "ai-buddy": entry() } }));
        }
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let mut root: Value = serde_json::from_str(&text).map_err(|_| {
        format!(
            "{} is not JSON, so the ai-buddy entry was left alone",
            path.display()
        )
    })?;
    let Some(object) = root.as_object_mut() else {
        return Err(format!(
            "{} is not an object, so the ai-buddy entry was left alone",
            path.display()
        ));
    };
    match object.get("mcpServers") {
        None => {
            object.insert("mcpServers".to_string(), json!({ "ai-buddy": entry() }));
        }
        Some(Value::Object(servers)) if servers.get("ai-buddy") == Some(&entry()) => return Ok(()),
        Some(Value::Object(_)) => {}
        Some(_) => {
            return Err(format!(
                "{} has mcpServers that is not an object, so it was left alone",
                path.display()
            ));
        }
    }
    let servers = object
        .get_mut("mcpServers")
        .and_then(Value::as_object_mut)
        .expect("mcpServers is an object here");
    servers.insert("ai-buddy".to_string(), entry());
    write(&path, &root)
}

fn write(path: &Path, value: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("ai-buddy-pi-mcp-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temp dir");
        path
    }

    #[test]
    fn a_missing_file_is_created_with_only_the_env_entry() {
        let dir = dir("create");
        sync_project_file(&dir).unwrap();
        let text = fs::read_to_string(dir.join(FILE)).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["mcpServers"]["ai-buddy"], entry());
        assert!(!text.contains("127.0.0.1"));
        assert!(!text.contains("Bearer"));
    }

    #[test]
    fn a_sibling_server_stays_and_a_literal_entry_is_replaced() {
        let dir = dir("sibling");
        let path = dir.join(FILE);
        fs::write(
            &path,
            r#"{"keep":true,"mcpServers":{"other":{"command":"echo"},"ai-buddy":{"url":"http://127.0.0.1:9/mcp","headers":{"Authorization":"Bearer secret"},"disabled":true}}}"#,
        )
        .unwrap();
        sync_project_file(&dir).unwrap();
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["keep"], true);
        assert_eq!(value["mcpServers"]["other"]["command"], "echo");
        assert_eq!(value["mcpServers"]["ai-buddy"], entry());
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("secret"));
        assert!(!text.contains("127.0.0.1"));
    }

    #[test]
    fn a_correct_entry_is_left_byte_for_byte() {
        let dir = dir("stable");
        let path = dir.join(FILE);
        let original = "{\n  \"mcpServers\": {\n    \"ai-buddy\": {\n      \"url\": \"${AI_BUDDY_MCP_URL}\",\n      \"auth\": \"bearer\",\n      \"bearerTokenEnv\": \"AI_BUDDY_MCP_TOKEN\",\n      \"lifecycle\": \"eager\"\n    }\n  }\n}\n";
        fs::write(&path, original).unwrap();
        sync_project_file(&dir).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn invalid_json_is_left_unchanged() {
        let dir = dir("invalid");
        let path = dir.join(FILE);
        fs::write(&path, "not json").unwrap();
        let error = sync_project_file(&dir).unwrap_err();
        assert!(error.contains("not JSON"), "{error}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "not json");
    }

    #[test]
    fn mcp_servers_of_the_wrong_type_is_left_unchanged() {
        let dir = dir("wrong-type");
        let path = dir.join(FILE);
        fs::write(&path, "{\"mcpServers\":[]}").unwrap();
        let error = sync_project_file(&dir).unwrap_err();
        assert!(error.contains("not an object"), "{error}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"mcpServers\":[]}");
    }

    #[test]
    fn a_missing_directory_is_an_error() {
        let dir = dir("missing-dir");
        let gone = dir.join("missing");
        let error = sync_project_file(&gone).unwrap_err();
        assert!(!gone.join(FILE).exists(), "{error}");
    }
}
