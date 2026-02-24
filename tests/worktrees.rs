//! TDD tests for git worktree integration.
//! RED: written before implementation.

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

    StdCommand::new("git").args(["checkout", "-b", "feat/big"]).current_dir(&root).output().unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(root.join(format!("src/{}", name)), format!("// {}", name)).unwrap();
    }
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "add files"]).current_dir(&root).output().unwrap();

    (dir, root)
}

fn write_state(root: &std::path::Path, use_worktrees: bool) {
    let state = serde_json::json!({
        "base_branch": "main",
        "source_branch": "feat/big",
        "repo_owner": "acme",
        "repo_name": "myrepo",
        "strategy": "stacked",
        "use_worktrees": use_worktrees,
        "chunks": []
    });
    std::fs::write(root.join(".merges.json"), serde_json::to_string_pretty(&state).unwrap()).unwrap();
}

// ── State tests ───────────────────────────────────────────────────────────────

/// State file with use_worktrees=true round-trips correctly.
#[test]
fn test_state_use_worktrees_roundtrip() {
    let (_dir, root) = make_repo();
    write_state(&root, true);
    let state = merges::state::MergesState::load(&root).unwrap();
    assert!(state.use_worktrees, "use_worktrees should be true");
}

/// State file without use_worktrees field defaults to false (backward compat).
#[test]
fn test_state_use_worktrees_defaults_false() {
    let (_dir, root) = make_repo();
    // Write state WITHOUT the use_worktrees field (old format)
    let state = serde_json::json!({
        "base_branch": "main",
        "source_branch": "feat/big",
        "repo_owner": "acme",
        "repo_name": "myrepo",
        "strategy": "stacked",
        "chunks": []
    });
    std::fs::write(root.join(".merges.json"), serde_json::to_string_pretty(&state).unwrap()).unwrap();

    let loaded = merges::state::MergesState::load(&root).unwrap();
    assert!(!loaded.use_worktrees, "use_worktrees should default to false");
}

// ── git::worktree_path tests ──────────────────────────────────────────────────

/// Worktree path is inside .git/merges-worktrees/<branch-name>.
#[test]
fn test_worktree_path_is_inside_git_dir() {
    let (_dir, root) = make_repo();
    let path = merges::git::worktree_path(&root, "feat/payments-v2-chunk-1-db");
    // Should be under .git/merges-worktrees/
    assert!(
        path.starts_with(root.join(".git").join("merges-worktrees")),
        "worktree path should be inside .git/merges-worktrees/, got: {:?}",
        path
    );
}

/// Slashes in branch names are replaced with dashes in the directory name.
#[test]
fn test_worktree_path_sanitises_branch_name() {
    let (_dir, root) = make_repo();
    let path = merges::git::worktree_path(&root, "feat/payments-v2-chunk-1-db");
    let dir_name = path.file_name().unwrap().to_str().unwrap();
    assert!(
        !dir_name.contains('/'),
        "directory name should not contain slashes, got: {}",
        dir_name
    );
}

// ── git::add_worktree / remove_worktree tests ─────────────────────────────────

/// add_worktree creates a new branch and a worktree directory.
#[test]
fn test_add_worktree_creates_directory() {
    let (_dir, root) = make_repo();
    let branch = "feat/big-chunk-1-models";
    let base = merges::git::merge_base(&root, "main").unwrap();

    merges::git::add_worktree(&root, branch, &base).unwrap();

    let wt_path = merges::git::worktree_path(&root, branch);
    assert!(wt_path.exists(), "worktree directory should exist at {:?}", wt_path);
}

/// add_worktree leaves the main worktree on the original branch.
#[test]
fn test_add_worktree_does_not_change_current_branch() {
    let (_dir, root) = make_repo();
    let base = merges::git::merge_base(&root, "main").unwrap();

    merges::git::add_worktree(&root, "feat/big-chunk-1-models", &base).unwrap();

    let branch = merges::git::current_branch(&root).unwrap();
    assert_eq!(branch, "feat/big", "main worktree branch should be unchanged");
}

/// remove_worktree deletes the directory and deregisters the worktree.
#[test]
fn test_remove_worktree_deletes_directory() {
    let (_dir, root) = make_repo();
    let branch = "feat/big-chunk-1-models";
    let base = merges::git::merge_base(&root, "main").unwrap();

    merges::git::add_worktree(&root, branch, &base).unwrap();
    let wt_path = merges::git::worktree_path(&root, branch);
    assert!(wt_path.exists());

    merges::git::remove_worktree(&root, branch).unwrap();
    assert!(!wt_path.exists(), "worktree directory should be gone after remove");
}

// ── apply_plan with worktrees ─────────────────────────────────────────────────

/// apply_plan with use_worktrees=true creates worktree dirs instead of checking out.
#[test]
fn test_apply_plan_worktrees_creates_worktree_dirs() {
    let (_dir, root) = make_repo();
    write_state(&root, true);

    merges::split::apply_plan(
        &root,
        vec![
            merges::split::ChunkPlan { name: "part-a".to_string(), files: vec!["src/a.rs".to_string()] },
            merges::split::ChunkPlan { name: "part-b".to_string(), files: vec!["src/b.rs".to_string()] },
        ],
    ).unwrap();

    // Both worktree dirs should exist
    let wt_a = merges::git::worktree_path(&root, "feat/big-chunk-1-part-a");
    let wt_b = merges::git::worktree_path(&root, "feat/big-chunk-2-part-b");
    assert!(wt_a.exists(), "worktree for part-a should exist");
    assert!(wt_b.exists(), "worktree for part-b should exist");
}

/// apply_plan with use_worktrees=true leaves main worktree on source branch.
#[test]
fn test_apply_plan_worktrees_does_not_switch_branch() {
    let (_dir, root) = make_repo();
    write_state(&root, true);

    merges::split::apply_plan(
        &root,
        vec![
            merges::split::ChunkPlan { name: "part-a".to_string(), files: vec!["src/a.rs".to_string()] },
        ],
    ).unwrap();

    let branch = merges::git::current_branch(&root).unwrap();
    assert_eq!(branch, "feat/big", "source branch should still be active");
}

/// Files in each worktree contain only the chunk's files.
#[test]
fn test_apply_plan_worktrees_each_chunk_has_correct_files() {
    let (_dir, root) = make_repo();
    write_state(&root, true);

    merges::split::apply_plan(
        &root,
        vec![
            merges::split::ChunkPlan { name: "part-a".to_string(), files: vec!["src/a.rs".to_string()] },
            merges::split::ChunkPlan { name: "part-b".to_string(), files: vec!["src/b.rs".to_string()] },
        ],
    ).unwrap();

    let wt_a = merges::git::worktree_path(&root, "feat/big-chunk-1-part-a");
    let mut files_a = merges::git::changed_files(&wt_a, "main").unwrap();
    files_a.sort();
    assert_eq!(files_a, vec!["src/a.rs"], "part-a worktree diff should only have src/a.rs");

    let wt_b = merges::git::worktree_path(&root, "feat/big-chunk-2-part-b");
    let mut files_b = merges::git::changed_files(&wt_b, "main").unwrap();
    files_b.sort();
    assert_eq!(files_b, vec!["src/b.rs"], "part-b worktree diff should only have src/b.rs");
}
