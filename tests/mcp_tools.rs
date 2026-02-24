//! TDD tests for missing MCP tools: merges_add, merges_move, merges_clean, merges_doctor.
//! RED: dispatch_tool must handle these names; they're currently "Unknown tool" → tests fail.
//!
//! NOTE: These tests mutate the process working directory and must run single-threaded.
//! Run with: cargo test --test mcp_tools -- --test-threads=1

use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

// ── helpers ───────────────────────────────────────────────────────────────────

fn make_repo_with_two_chunks() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();

    StdCommand::new("git").args(["init", "-b", "main"]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["config", "user.name", "T"]).current_dir(&root).output().unwrap();

    fs::write(root.join("README.md"), "root").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "init"]).current_dir(&root).output().unwrap();

    // Feature branch with files
    StdCommand::new("git").args(["checkout", "-b", "feat/big"]).current_dir(&root).output().unwrap();
    fs::create_dir_all(root.join("src/models")).unwrap();
    fs::create_dir_all(root.join("src/api")).unwrap();
    fs::write(root.join("src/models/user.rs"), "struct User;").unwrap();
    fs::write(root.join("src/models/post.rs"), "struct Post;").unwrap();
    fs::write(root.join("src/api/routes.rs"), "fn routes() {}").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "add src"]).current_dir(&root).output().unwrap();

    // Write state with two chunks and create branches
    let state = serde_json::json!({
        "base_branch": "main",
        "source_branch": "feat/big",
        "repo_owner": "acme",
        "repo_name": "myrepo",
        "strategy": "independent",
        "use_worktrees": false,
        "chunks": [
            {
                "name": "models",
                "branch": "feat/big-chunk-models",
                "files": ["src/models/user.rs", "src/models/post.rs"],
                "pr_number": null,
                "pr_url": null,
                "status": "pending"
            },
            {
                "name": "api",
                "branch": "feat/big-chunk-api",
                "files": ["src/api/routes.rs"],
                "pr_number": null,
                "pr_url": null,
                "status": "pending"
            }
        ]
    });
    fs::write(root.join(".merges.json"), serde_json::to_string_pretty(&state).unwrap()).unwrap();
    merges::git::ensure_gitignored(&root, ".merges.json").unwrap();

    for branch in &["feat/big-chunk-models", "feat/big-chunk-api"] {
        StdCommand::new("git").args(["checkout", "-b", branch]).current_dir(&root).output().unwrap();
        // commit a file on each branch so they diverge
        StdCommand::new("git").args(["checkout", "feat/big"]).current_dir(&root).output().unwrap();
    }

    (dir, root)
}

// ── merges_doctor MCP tool ────────────────────────────────────────────────────

/// merges_doctor returns a JSON report with an "all_ok" boolean field.
#[test]
fn test_mcp_doctor_returns_json_report() {
    let (_dir, root) = make_repo_with_two_chunks();
    std::env::set_current_dir(&root).unwrap();

    let result = merges::mcp::call_tool_sync("merges_doctor", &serde_json::json!({}));
    assert!(result.is_ok(), "merges_doctor should not error: {:?}", result);

    let text = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&text)
        .expect("merges_doctor should return valid JSON");
    assert!(parsed.get("all_ok").is_some(), "Report must have 'all_ok' field: {}", text);
}

/// merges_doctor --repair flag is accepted without error.
#[test]
fn test_mcp_doctor_repair_flag_accepted() {
    let (_dir, root) = make_repo_with_two_chunks();
    std::env::set_current_dir(&root).unwrap();

    let result = merges::mcp::call_tool_sync("merges_doctor", &serde_json::json!({"repair": true}));
    assert!(result.is_ok(), "merges_doctor repair should not error: {:?}", result);
}

// ── merges_clean MCP tool ─────────────────────────────────────────────────────

/// merges_clean dry_run returns a JSON list of branches that would be deleted.
#[test]
fn test_mcp_clean_dry_run_returns_branch_list() {
    let (_dir, root) = make_repo_with_two_chunks();
    std::env::set_current_dir(&root).unwrap();

    let result = merges::mcp::call_tool_sync("merges_clean", &serde_json::json!({"dry_run": true}));
    assert!(result.is_ok(), "merges_clean dry_run should not error: {:?}", result);

    let text = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&text)
        .expect("merges_clean should return valid JSON");
    assert!(parsed.get("branches").is_some(), "Response must have 'branches' field: {}", text);
}

// ── merges_add MCP tool ───────────────────────────────────────────────────────

/// merges_add with missing chunk name returns a clear error (not "Unknown tool").
#[test]
fn test_mcp_add_unknown_chunk_returns_error_not_unknown_tool() {
    let (_dir, root) = make_repo_with_two_chunks();
    std::env::set_current_dir(&root).unwrap();

    let result = merges::mcp::call_tool_sync(
        "merges_add",
        &serde_json::json!({"chunk": "no-such-chunk", "files": ["src/models/user.rs"]}),
    );
    // Should error with a domain error (chunk not found), not "Unknown tool"
    let err_msg = result.unwrap_err().to_string();
    assert!(
        !err_msg.contains("Unknown tool"),
        "Should dispatch to merges_add, got: {}", err_msg
    );
}

// ── merges_move MCP tool ──────────────────────────────────────────────────────

/// merges_move with bad chunk returns a domain error, not "Unknown tool".
#[test]
fn test_mcp_move_unknown_chunk_returns_error_not_unknown_tool() {
    let (_dir, root) = make_repo_with_two_chunks();
    std::env::set_current_dir(&root).unwrap();

    let result = merges::mcp::call_tool_sync(
        "merges_move",
        &serde_json::json!({
            "file": "src/models/user.rs",
            "from": "no-such-chunk",
            "to": "api"
        }),
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        !err_msg.contains("Unknown tool"),
        "Should dispatch to merges_move, got: {}", err_msg
    );
}
