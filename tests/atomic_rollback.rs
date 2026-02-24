//! TDD tests for atomic rollback in apply_plan.
//! RED: these tests describe desired behaviour before the implementation exists.

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

    // Base commit
    std::fs::write(root.join("README.md"), "hello").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "init"]).current_dir(&root).output().unwrap();

    // Feature branch
    StdCommand::new("git").args(["checkout", "-b", "feat/big"]).current_dir(&root).output().unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(root.join(format!("src/{}", name)), format!("// {}", name)).unwrap();
    }
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "add files"]).current_dir(&root).output().unwrap();

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

/// ❌ RED: When apply_plan fails partway through, no chunk branches should be left
/// and the state file should be unchanged.
#[test]
fn test_apply_plan_rolls_back_on_partial_failure() {
    let (_dir, root) = make_repo_with_changes();
    write_state(&root);

    // Plan: first chunk is valid, second chunk has a nonexistent file → will fail validation
    let plan = vec![
        merges::split::ChunkPlan {
            name: "valid".to_string(),
            files: vec!["src/a.rs".to_string()],
        },
        merges::split::ChunkPlan {
            name: "invalid".to_string(),
            files: vec!["src/does_not_exist.rs".to_string()], // not in diff → triggers error
        },
    ];

    let result = merges::split::apply_plan(&root, plan);
    assert!(result.is_err(), "apply_plan should fail due to invalid file");

    // ── State must be unchanged (no chunks saved) ──────────────────────────
    let state = merges::state::MergesState::load(&root).unwrap();
    assert_eq!(
        state.chunks.len(),
        0,
        "State should be unchanged after rollback, found: {:?}",
        state.chunks.iter().map(|c| &c.name).collect::<Vec<_>>()
    );

    // ── No chunk branches should remain ───────────────────────────────────
    let branches = StdCommand::new("git")
        .args(["branch", "--list"])
        .current_dir(&root)
        .output()
        .unwrap();
    let branch_list = String::from_utf8_lossy(&branches.stdout);
    assert!(
        !branch_list.contains("chunk"),
        "No chunk branches should remain after rollback, found:\n{}",
        branch_list
    );
}

/// ❌ RED: The source branch should be the active branch after a failed apply_plan.
#[test]
fn test_apply_plan_rollback_restores_source_branch() {
    let (_dir, root) = make_repo_with_changes();
    write_state(&root);

    let plan = vec![
        merges::split::ChunkPlan {
            name: "good".to_string(),
            files: vec!["src/a.rs".to_string()],
        },
        merges::split::ChunkPlan {
            name: "bad".to_string(),
            files: vec!["src/nonexistent.rs".to_string()],
        },
    ];

    let _ = merges::split::apply_plan(&root, plan);

    let branch = merges::git::current_branch(&root).unwrap();
    assert_eq!(
        branch, "feat/big",
        "Source branch should be restored after rollback"
    );
}

/// Successful plan leaves the state with all chunks.
#[test]
fn test_apply_plan_success_commits_all_chunks() {
    let (_dir, root) = make_repo_with_changes();
    write_state(&root);

    let plan = vec![
        merges::split::ChunkPlan { name: "part-a".to_string(), files: vec!["src/a.rs".to_string()] },
        merges::split::ChunkPlan { name: "part-b".to_string(), files: vec!["src/b.rs".to_string(), "src/c.rs".to_string()] },
    ];

    merges::split::apply_plan(&root, plan).unwrap();

    let state = merges::state::MergesState::load(&root).unwrap();
    assert_eq!(state.chunks.len(), 2);
}

/// ❌ RED: When a failure happens MID-LOOP (after chunk 1 branch was created),
/// the already-created chunk-1 branch must be cleaned up.
#[test]
fn test_apply_plan_rolls_back_mid_loop_failure() {
    let (_dir, root) = make_repo_with_changes();
    write_state(&root);

    // Pre-create the branch that chunk 2 would use — so it fails mid-loop
    let expected_chunk2_branch = "feat/big-chunk-2-second";
    StdCommand::new("git")
        .args(["branch", expected_chunk2_branch])
        .current_dir(&root)
        .output()
        .unwrap();

    let plan = vec![
        merges::split::ChunkPlan {
            name: "first".to_string(),
            files: vec!["src/a.rs".to_string()],
        },
        merges::split::ChunkPlan {
            name: "second".to_string(), // branch already exists → create_branch will fail
            files: vec!["src/b.rs".to_string()],
        },
    ];

    let result = merges::split::apply_plan(&root, plan);
    assert!(result.is_err(), "apply_plan should fail because chunk-2 branch exists");

    // ── chunk-1 branch should be cleaned up ──────────────────────────────
    let branches = StdCommand::new("git")
        .args(["branch", "--list"])
        .current_dir(&root)
        .output()
        .unwrap();
    let branch_list = String::from_utf8_lossy(&branches.stdout);
    assert!(
        !branch_list.contains("chunk-1-first"),
        "chunk-1-first branch should be rolled back after mid-loop failure, branches:\n{}",
        branch_list
    );

    // ── State should be unchanged ─────────────────────────────────────────
    let state = merges::state::MergesState::load(&root).unwrap();
    assert_eq!(state.chunks.len(), 0, "State must be unchanged after rollback");

    // ── Active branch should be restored ─────────────────────────────────
    let branch = merges::git::current_branch(&root).unwrap();
    assert_eq!(branch, "feat/big");
}
