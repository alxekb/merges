//! Integration tests for non-interactive split (TDD — RED first).
//! These tests FAIL until the implementation is in place.

use std::process::Command as StdCommand;
use tempfile::TempDir;

fn make_repo_with_changes() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();

    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "Test"],
    ] {
        StdCommand::new("git").args(&args).current_dir(&root).output().unwrap();
    }

    // Base commit on main
    std::fs::write(root.join("README.md"), "hello").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "init"]).current_dir(&root).output().unwrap();

    // Feature branch with multiple files
    StdCommand::new("git").args(["checkout", "-b", "feat/big"]).current_dir(&root).output().unwrap();
    std::fs::create_dir_all(root.join("src/models")).unwrap();
    std::fs::create_dir_all(root.join("src/api")).unwrap();
    std::fs::write(root.join("src/models/user.rs"), "struct User;").unwrap();
    std::fs::write(root.join("src/models/post.rs"), "struct Post;").unwrap();
    std::fs::write(root.join("src/api/routes.rs"), "fn routes() {}").unwrap();
    std::fs::write(root.join("src/api/handlers.rs"), "fn handle() {}").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "add feature files"]).current_dir(&root).output().unwrap();

    (dir, root)
}

fn write_state(root: &std::path::Path) {
    let state = serde_json::json!({
        "base_branch": "main",
        "source_branch": "feat/big",
        "repo_owner": "acme",
        "repo_name": "myrepo",
        "strategy": "stacked",
        "chunks": []
    });
    std::fs::write(
        root.join(".merges.json"),
        serde_json::to_string_pretty(&state).unwrap(),
    )
    .unwrap();
}

/// A chunk plan as JSON — this is what an LLM would produce.
fn chunk_plan_json() -> String {
    serde_json::to_string(&serde_json::json!([
        {
            "name": "models",
            "files": ["src/models/user.rs", "src/models/post.rs"]
        },
        {
            "name": "api",
            "files": ["src/api/routes.rs", "src/api/handlers.rs"]
        }
    ]))
    .unwrap()
}

// ── Tests for the apply_plan function ────────────────────────────────────────

#[test]
fn test_apply_plan_creates_chunk_branches() {
    let (_dir, root) = make_repo_with_changes();
    write_state(&root);

    let plan: Vec<merges::split::ChunkPlan> =
        serde_json::from_str(&chunk_plan_json()).unwrap();
    merges::split::apply_plan(&root, plan).unwrap();

    // Branches should exist
    let branches = StdCommand::new("git")
        .args(["branch", "--list"])
        .current_dir(&root)
        .output()
        .unwrap();
    let branch_list = String::from_utf8_lossy(&branches.stdout);
    assert!(branch_list.contains("feat/big-chunk-1-models"), "chunk-1-models branch missing");
    assert!(branch_list.contains("feat/big-chunk-2-api"), "chunk-2-api branch missing");
}

#[test]
fn test_apply_plan_returns_to_source_branch() {
    let (_dir, root) = make_repo_with_changes();
    write_state(&root);

    let plan: Vec<merges::split::ChunkPlan> =
        serde_json::from_str(&chunk_plan_json()).unwrap();
    merges::split::apply_plan(&root, plan).unwrap();

    let branch = merges::git::current_branch(&root).unwrap();
    assert_eq!(branch, "feat/big", "Should return to source branch after split");
}

#[test]
fn test_apply_plan_updates_state_file() {
    let (_dir, root) = make_repo_with_changes();
    write_state(&root);

    let plan: Vec<merges::split::ChunkPlan> =
        serde_json::from_str(&chunk_plan_json()).unwrap();
    merges::split::apply_plan(&root, plan).unwrap();

    let state = merges::state::MergesState::load(&root).unwrap();
    assert_eq!(state.chunks.len(), 2);
    assert_eq!(state.chunks[0].name, "models");
    assert_eq!(state.chunks[1].name, "api");
    assert_eq!(state.chunks[0].files, vec!["src/models/user.rs", "src/models/post.rs"]);
}

#[test]
fn test_apply_plan_chunk_branches_contain_correct_files() {
    let (_dir, root) = make_repo_with_changes();
    write_state(&root);

    let plan: Vec<merges::split::ChunkPlan> =
        serde_json::from_str(&chunk_plan_json()).unwrap();
    merges::split::apply_plan(&root, plan).unwrap();

    // Check out the models chunk branch and verify only model files exist
    merges::git::checkout(&root, "feat/big-chunk-1-models").unwrap();
    let files = merges::git::changed_files(&root, "main").unwrap();
    let mut files_sorted = files.clone();
    files_sorted.sort();
    assert_eq!(
        files_sorted,
        vec!["src/models/post.rs", "src/models/user.rs"],
        "models chunk should only contain model files, got: {:?}",
        files
    );
}

#[test]
fn test_apply_plan_empty_plan_returns_error() {
    let (_dir, root) = make_repo_with_changes();
    write_state(&root);

    let result = merges::split::apply_plan(&root, vec![]);
    assert!(result.is_err(), "Empty plan should return an error");
}

#[test]
fn test_apply_plan_file_not_in_diff_returns_error() {
    let (_dir, root) = make_repo_with_changes();
    write_state(&root);

    let plan = vec![merges::split::ChunkPlan {
        name: "nonexistent".to_string(),
        files: vec!["does/not/exist.rs".to_string()],
    }];
    let result = merges::split::apply_plan(&root, plan);
    assert!(result.is_err(), "Plan with files not in diff should fail");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("does/not/exist.rs"),
        "Error should name the bad file, got: {}",
        msg
    );
}

// ── Duplicate file validation ─────────────────────────────────────────────────

/// apply_plan should reject a plan where a file is already assigned to an existing chunk.
#[test]
fn test_apply_plan_rejects_file_already_in_existing_chunk() {
    let (_dir, root) = make_repo_with_changes();
    write_state(&root);

    // First split: src/models/user.rs → first chunk
    merges::split::apply_plan(&root, vec![
        merges::split::ChunkPlan {
            name: "first".to_string(),
            files: vec!["src/models/user.rs".to_string()],
        },
    ]).unwrap();

    // Second split: try to put src/models/user.rs into another chunk
    let result = merges::split::apply_plan(&root, vec![
        merges::split::ChunkPlan {
            name: "second".to_string(),
            files: vec!["src/models/user.rs".to_string()],
        },
    ]);

    assert!(result.is_err(), "Should reject file already in an existing chunk");
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("src/models/user.rs"), "Error should name the already-assigned file: {}", msg);
}

/// apply_plan rejects a plan with a duplicate within a single chunk.
#[test]
fn test_apply_plan_rejects_duplicate_file_within_chunk() {
    let (_dir, root) = make_repo_with_changes();
    write_state(&root);

    let result = merges::split::apply_plan(&root, vec![
        merges::split::ChunkPlan {
            name: "a".to_string(),
            files: vec!["src/models/user.rs".to_string(), "src/models/user.rs".to_string()],
        },
    ]);

    assert!(result.is_err(), "Should reject duplicate within a single chunk");
}

/// apply_plan should reject a plan where the same file appears in two chunks.
#[test]
fn test_apply_plan_rejects_duplicate_file_across_chunks() {
    let (_dir, root) = make_repo_with_changes();
    write_state(&root);

    let result = merges::split::apply_plan(&root, vec![
        merges::split::ChunkPlan { name: "a".to_string(), files: vec!["src/models/user.rs".to_string()] },
        merges::split::ChunkPlan { name: "b".to_string(), files: vec!["src/models/user.rs".to_string()] },
    ]);

    assert!(result.is_err(), "Should reject duplicate file across chunks");
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("src/models/user.rs"), "Error should name the duplicate file: {}", msg);
}
