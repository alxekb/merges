//! TDD tests for `merges move` — move a file from one chunk to another.
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
        StdCommand::new("git")
            .args(&args)
            .current_dir(&root)
            .output()
            .unwrap();
    }

    std::fs::write(root.join("README.md"), "hello").unwrap();
    StdCommand::new("git")
        .args(["add", "."])
        .current_dir(&root)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&root)
        .output()
        .unwrap();

    // Feature branch with several files
    StdCommand::new("git")
        .args(["checkout", "-b", "feat/big"])
        .current_dir(&root)
        .output()
        .unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    for name in ["a.rs", "b.rs", "c.rs", "d.rs"] {
        std::fs::write(
            root.join(format!("src/{}", name)),
            format!("// {}", name),
        )
        .unwrap();
    }
    StdCommand::new("git")
        .args(["add", "."])
        .current_dir(&root)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["commit", "-m", "add files"])
        .current_dir(&root)
        .output()
        .unwrap();

    (dir, root)
}

fn setup_two_chunks(root: &std::path::Path) {
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

    merges::split::apply_plan(
        root,
        vec![
            merges::split::ChunkPlan {
                name: "chunk-a".to_string(),
                files: vec!["src/a.rs".to_string(), "src/b.rs".to_string()],
            },
            merges::split::ChunkPlan {
                name: "chunk-b".to_string(),
                files: vec!["src/c.rs".to_string()],
            },
        ],
    )
    .unwrap();
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Moving a file should remove it from the source chunk and add it to dest.
#[test]
fn test_move_removes_from_source_chunk() {
    let (_dir, root) = make_repo();
    setup_two_chunks(&root);

    merges::commands::r#move::run(&root, "src/b.rs", "chunk-a", "chunk-b").unwrap();

    let state = merges::state::MergesState::load(&root).unwrap();
    let chunk_a = state.chunks.iter().find(|c| c.name == "chunk-a").unwrap();
    assert!(
        !chunk_a.files.contains(&"src/b.rs".to_string()),
        "File should be removed from source chunk, files: {:?}",
        chunk_a.files
    );
}

/// Moving a file should add it to the destination chunk.
#[test]
fn test_move_adds_to_dest_chunk() {
    let (_dir, root) = make_repo();
    setup_two_chunks(&root);

    merges::commands::r#move::run(&root, "src/b.rs", "chunk-a", "chunk-b").unwrap();

    let state = merges::state::MergesState::load(&root).unwrap();
    let chunk_b = state.chunks.iter().find(|c| c.name == "chunk-b").unwrap();
    assert!(
        chunk_b.files.contains(&"src/b.rs".to_string()),
        "File should be in dest chunk, files: {:?}",
        chunk_b.files
    );
}

/// Source chunk branch diff should not contain the moved file.
#[test]
fn test_move_source_branch_no_longer_has_file() {
    let (_dir, root) = make_repo();
    setup_two_chunks(&root);

    merges::commands::r#move::run(&root, "src/b.rs", "chunk-a", "chunk-b").unwrap();

    merges::git::checkout(&root, "feat/big-chunk-1-chunk-a").unwrap();
    let files = merges::git::changed_files(&root, "main").unwrap();
    assert!(
        !files.contains(&"src/b.rs".to_string()),
        "src/b.rs should no longer be in chunk-a branch diff, diff: {:?}",
        files
    );
}

/// Dest chunk branch diff should contain the moved file.
#[test]
fn test_move_dest_branch_has_file() {
    let (_dir, root) = make_repo();
    setup_two_chunks(&root);

    merges::commands::r#move::run(&root, "src/b.rs", "chunk-a", "chunk-b").unwrap();

    merges::git::checkout(&root, "feat/big-chunk-2-chunk-b").unwrap();
    let mut files = merges::git::changed_files(&root, "main").unwrap();
    files.sort();
    assert_eq!(
        files,
        vec!["src/b.rs", "src/c.rs"],
        "dest chunk should have both original and moved file"
    );
}

/// Restores source branch after move.
#[test]
fn test_move_restores_source_branch() {
    let (_dir, root) = make_repo();
    setup_two_chunks(&root);

    merges::commands::r#move::run(&root, "src/b.rs", "chunk-a", "chunk-b").unwrap();

    let branch = merges::git::current_branch(&root).unwrap();
    assert_eq!(branch, "feat/big", "Source branch should be restored after move");
}

/// Moving a file not in the source chunk returns an error.
#[test]
fn test_move_file_not_in_source_chunk_errors() {
    let (_dir, root) = make_repo();
    setup_two_chunks(&root);

    let result = merges::commands::r#move::run(&root, "src/c.rs", "chunk-a", "chunk-b");
    assert!(result.is_err(), "Should fail when file is not in source chunk");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("src/c.rs") || msg.contains("chunk-a"),
        "Error should name the file or chunk: {}",
        msg
    );
}

/// Moving a file to a nonexistent chunk returns an error.
#[test]
fn test_move_to_nonexistent_chunk_errors() {
    let (_dir, root) = make_repo();
    setup_two_chunks(&root);

    let result = merges::commands::r#move::run(&root, "src/b.rs", "chunk-a", "no-such-chunk");
    assert!(result.is_err(), "Should fail when dest chunk doesn't exist");
}

/// Moving from a nonexistent chunk returns an error.
#[test]
fn test_move_from_nonexistent_chunk_errors() {
    let (_dir, root) = make_repo();
    setup_two_chunks(&root);

    let result = merges::commands::r#move::run(&root, "src/b.rs", "no-such-chunk", "chunk-b");
    assert!(result.is_err(), "Should fail when src chunk doesn't exist");
}
