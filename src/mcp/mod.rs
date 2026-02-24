//! Minimal stdio-based MCP server (JSON-RPC 2.0).
//!
//! The server reads newline-delimited JSON from stdin and writes responses to stdout.
//! This is compatible with the Model Context Protocol used by Claude, GitHub Copilot, and others.

pub mod tools;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::{
    commands, doctor, git,
    state::MergesState,
};

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)] // parsed for spec compliance; not used further
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

impl JsonRpcResponse {
    fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn err(id: Value, code: i64, message: &str) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(json!({"code": code, "message": message})),
        }
    }
}

pub async fn run() -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin).lines();
    let mut stdout = stdout;

    eprintln!("merges MCP server running on stdio (JSON-RPC 2.0)");

    while let Some(line) = reader.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Err(e) => JsonRpcResponse::err(
                Value::Null,
                -32700,
                &format!("Parse error: {}", e),
            ),
            Ok(req) => {
                let id = req.id.clone().unwrap_or(Value::Null);
                handle_request(req).await.unwrap_or_else(|e| {
                    JsonRpcResponse::err(id, -32000, &e.to_string())
                })
            }
        };

        let mut out = serde_json::to_string(&response)?;
        out.push('\n');
        stdout.write_all(out.as_bytes()).await?;
        stdout.flush().await?;
    }

    Ok(())
}

async fn handle_request(req: JsonRpcRequest) -> Result<JsonRpcResponse> {
    let id = req.id.unwrap_or(Value::Null);

    match req.method.as_str() {
        // MCP lifecycle
        "initialize" => Ok(JsonRpcResponse::ok(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "merges",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )),

        "notifications/initialized" => {
            // No response needed for notifications
            Ok(JsonRpcResponse::ok(id, json!({})))
        }

        "tools/list" => Ok(JsonRpcResponse::ok(
            id,
            json!({ "tools": tools::all_tools() }),
        )),

        "tools/call" => {
            let params = req.params.unwrap_or(json!({}));
            let tool_name = params["name"].as_str().unwrap_or("").to_string();
            let args = params.get("arguments").cloned().unwrap_or(json!({}));

            let result = dispatch_tool(&tool_name, &args).await?;
            Ok(JsonRpcResponse::ok(
                id,
                json!({
                    "content": [{
                        "type": "text",
                        "text": result
                    }]
                }),
            ))
        }

        other => Ok(JsonRpcResponse::err(
            id,
            -32601,
            &format!("Method not found: {}", other),
        )),
    }
}

async fn dispatch_tool(name: &str, args: &Value) -> Result<String> {
    match name {
        "merges_init" => {
            let base = args.get("base_branch").and_then(|v| v.as_str()).map(String::from);
            commands::init::run(base, false)?;
            Ok("Initialised successfully.".to_string())
        }

        "merges_split" => {
            let root = git::repo_root()?;
            let state = MergesState::load(&root)?;

            if let Some(plan_val) = args.get("plan") {
                // LLM provided a plan — apply it non-interactively
                let plan: Vec<crate::split::ChunkPlan> =
                    serde_json::from_value(plan_val.clone())
                        .map_err(|e| anyhow::anyhow!("Invalid plan format: {}", e))?;
                crate::split::apply_plan(&root, plan)?;
                let updated = MergesState::load(&root)?;
                Ok(serde_json::to_string_pretty(&json!({
                    "status": "applied",
                    "chunks_created": updated.chunks.len(),
                    "chunks": updated.chunks.iter().map(|c| json!({
                        "name": c.name,
                        "branch": c.branch,
                        "files": c.files
                    })).collect::<Vec<_>>()
                }))?)
            } else {
                // No plan yet — return files so the LLM can decide how to split
                let files = crate::git::changed_files(&root, &state.base_branch)?;
                Ok(serde_json::to_string_pretty(&json!({
                    "changed_files": files,
                    "instructions": "Call merges_split again with a 'plan' field: [{\"name\":\"chunk-name\",\"files\":[\"path/to/file.rs\"]}]"
                }))?)
            }
        }

        "merges_push" => {
            let stacked = args.get("strategy").and_then(|v| v.as_str()) == Some("stacked");
            let independent = args.get("strategy").and_then(|v| v.as_str()) == Some("independent");
            commands::push::run(stacked, independent).await?;
            Ok("Push completed.".to_string())
        }

        "merges_sync" => {
            commands::sync::run()?;
            Ok("Sync completed.".to_string())
        }

        "merges_status" => {
            let root = git::repo_root()?;
            let state = MergesState::load(&root)?;
            Ok(serde_json::to_string_pretty(&json!({
                "source_branch": state.source_branch,
                "base_branch": state.base_branch,
                "strategy": state.strategy,
                "chunks": state.chunks.iter().map(|c| json!({
                    "name": c.name,
                    "branch": c.branch,
                    "files_count": c.files.len(),
                    "pr_number": c.pr_number,
                    "pr_url": c.pr_url
                })).collect::<Vec<_>>()
            }))?)
        }

        "merges_add" => {
            let root = git::repo_root()?;
            let chunk = args["chunk"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("'chunk' is required"))?
                .to_string();
            let files: Vec<String> = args["files"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("'files' must be an array"))?
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            commands::add::run(&root, &chunk, &files)?;
            Ok(serde_json::to_string_pretty(&json!({
                "status": "ok",
                "chunk": chunk,
                "files_added": files
            }))?)
        }

        "merges_move" => {
            let root = git::repo_root()?;
            let file = args["file"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("'file' is required"))?
                .to_string();
            let from = args["from"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("'from' is required"))?
                .to_string();
            let to = args["to"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("'to' is required"))?
                .to_string();
            commands::r#move::run(&root, &file, &from, &to)?;
            Ok(serde_json::to_string_pretty(&json!({
                "status": "ok",
                "file": file,
                "from": from,
                "to": to
            }))?)
        }

        "merges_clean" => {
            let root = git::repo_root()?;
            let state = MergesState::load(&root)?;
            let dry_run = args.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);
            let branches: Vec<String> = state.chunks.iter().map(|c| c.branch.clone()).collect();

            if dry_run {
                Ok(serde_json::to_string_pretty(&json!({
                    "dry_run": true,
                    "branches": branches
                }))?)
            } else {
                let merged = args.get("merged").and_then(|v| v.as_bool()).unwrap_or(false);
                commands::clean::run(merged, true).await?;
                Ok(serde_json::to_string_pretty(&json!({
                    "status": "ok",
                    "deleted": branches
                }))?)
            }
        }

        "merges_doctor" => {
            let root = git::repo_root()?;
            let repair = args.get("repair").and_then(|v| v.as_bool()).unwrap_or(false);
            let report = doctor::run(&root, repair)?;
            Ok(serde_json::to_string_pretty(&json!({
                "all_ok": report.all_ok(),
                "issues": report.issues
            }))?)
        }

        other => anyhow::bail!("Unknown tool: {}", other),
    }
}

/// Synchronous wrapper around `dispatch_tool` for use in integration tests.
#[allow(dead_code)]
pub fn call_tool_sync(name: &str, args: &serde_json::Value) -> anyhow::Result<String> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(dispatch_tool(name, args))
}
