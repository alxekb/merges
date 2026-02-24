//! TDD tests for `merges add` — add files to an existing chunk.
//! RED: tests written before the command exists.

use std::process::Command as StdCommand;
use tempfile::TempDir;

fn make_repo() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();

    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "Test"],
    ] {
        StdCommand::new("git").args(&args).current_dir(&root).output().unwrap();
    }

    std::fs::write(root.join("README.md"), "hello").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "init"]).current_dir(&root).output().unwrap();

    // Feature branch with several files
    StdCommand::new("git").args(["checkout", "-b", "feat/big"]).current_dir(&root).output().unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    for name in ["a.rs", "b.rs", "c.rs", "d.rs"] {
        std::fs::write(root.join(format!("src/{}", name)), format!("// {}", name)).unwrap();
    }
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "add files"]).current_dir(&root).output().unwrap();

    (dir, root)
}

fn setup_with_chunk(root: &std::path::Path) {
    // Write .merges.json
    let state = serde_json::json!({
        "base_branch": "main",
        "source_branch": "feat/big",
        "repo_owner": "acme",
        "repo_name": "myrepo",
        "strategy": "stacked",
        "chunks": []
    });
    std::fs::write(root.join(".merges.json"), serde_json::to_string_pretty(&state).unwrap()).unwrap();

    // Create chunk-1 with only src/a.rs
    merges::split::apply_plan(root, vec![
        merges::split::ChunkPlan { name: "part-a".to_string(), files: vec!["src/a.rs".to_string()] },
    ]).unwrap();
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// Adding a file to a chunk should make it appear in the chunk branch diff.
#[test]
fn test_add_file_to_existing_chunk() {
    let (_dir, root) = make_repo();
    setup_with_chunk(&root);

    merges::commands::add::run(&root, "part-a", &["src/b.rs".to_string()]).unwrap();

    // Check out chunk branch and verify both files are present
    merges::git::checkout(&root, "feat/big-chunk-1-part-a").unwrap();
    let mut files = merges::git::changed_files(&root, "main").unwrap();
    files.sort();
    assert_eq!(files, vec!["src/a.rs", "src/b.rs"],
        "Both original and newly added file should be in chunk");
}

/// State file should reflect the added file.
#[test]
fn test_add_updates_state_file() {
    let (_dir, root) = make_repo();
    setup_with_chunk(&root);

    merges::commands::add::run(&root, "part-a", &["src/b.rs".to_string()]).unwrap();

    let state = merges::state::MergesState::load(&root).unwrap();
    let chunk = state.chunks.iter().find(|c| c.name == "part-a").unwrap();
    assert!(chunk.files.contains(&"src/b.rs".to_string()),
        "State should include newly added file, files: {:?}", chunk.files);
}

/// Adding a file already in the chunk should be idempotent (not duplicate).
#[test]
fn test_add_idempotent_for_existing_file() {
    let (_dir, root) = make_repo();
    setup_with_chunk(&root);

    // src/a.rs is already in the chunk
    merges::commands::add::run(&root, "part-a", &["src/a.rs".to_string()]).unwrap();

    let state = merges::state::MergesState::load(&root).unwrap();
    let chunk = state.chunks.iter().find(|c| c.name == "part-a").unwrap();
    let count = chunk.files.iter().filter(|f| *f == "src/a.rs").count();
    assert_eq!(count, 1, "File should appear exactly once even after duplicate add");
}

/// Adding a file not in the source branch diff should return an error.
#[test]
fn test_add_file_not_in_diff_returns_error() {
    let (_dir, root) = make_repo();
    setup_with_chunk(&root);

    let result = merges::commands::add::run(&root, "part-a", &["src/nonexistent.rs".to_string()]);
    assert!(result.is_err(), "Adding nonexistent file should fail");
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("nonexistent.rs"), "Error should name the bad file: {}", msg);
}

/// Adding to a chunk that doesn't exist should return an error.
#[test]
fn test_add_to_nonexistent_chunk_returns_error() {
    let (_dir, root) = make_repo();
    setup_with_chunk(&root);

    let result = merges::commands::add::run(&root, "no-such-chunk", &["src/b.rs".to_string()]);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("no-such-chunk"), "Error should name the missing chunk: {}", msg);
}

/// After add, the source branch should still be active.
#[test]
fn test_add_restores_source_branch() {
    let (_dir, root) = make_repo();
    setup_with_chunk(&root);

    merges::commands::add::run(&root, "part-a", &["src/c.rs".to_string()]).unwrap();

    let branch = merges::git::current_branch(&root).unwrap();
    assert_eq!(branch, "feat/big", "Source branch should be active after add");
}

/// Adding multiple files at once should work.
#[test]
fn test_add_multiple_files_at_once() {
    let (_dir, root) = make_repo();
    setup_with_chunk(&root);

    merges::commands::add::run(&root, "part-a", &["src/b.rs".to_string(), "src/c.rs".to_string()]).unwrap();

    let state = merges::state::MergesState::load(&root).unwrap();
    let chunk = state.chunks.iter().find(|c| c.name == "part-a").unwrap();
    assert!(chunk.files.contains(&"src/b.rs".to_string()));
    assert!(chunk.files.contains(&"src/c.rs".to_string()));
}
