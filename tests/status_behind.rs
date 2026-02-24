//! TDD tests for `git::commits_behind` — RED phase.

use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

/// Create a git repo where `branch` is N commits behind `base`.
fn make_repo_with_divergence(commits_ahead_on_base: u32) -> (TempDir, std::path::PathBuf, String) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();

    StdCommand::new("git").args(["init", "-b", "main"]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["config", "user.name", "T"]).current_dir(&root).output().unwrap();

    // Initial commit on main
    fs::write(root.join("base.txt"), "base").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "base"]).current_dir(&root).output().unwrap();

    // Create feature branch from here
    StdCommand::new("git").args(["checkout", "-b", "feat/chunk-1"]).current_dir(&root).output().unwrap();
    fs::write(root.join("chunk.txt"), "chunk").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "chunk work"]).current_dir(&root).output().unwrap();

    // Go back to main, add N more commits (simulating base moving ahead)
    StdCommand::new("git").args(["checkout", "main"]).current_dir(&root).output().unwrap();
    for i in 0..commits_ahead_on_base {
        fs::write(root.join(format!("main_{i}.txt")), format!("main {i}")).unwrap();
        StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
        StdCommand::new("git")
            .args(["commit", "-m", &format!("main commit {i}")])
            .current_dir(&root)
            .output().unwrap();
    }

    (dir, root, "feat/chunk-1".to_string())
}

// ── commits_behind tests ──────────────────────────────────────────────────────

/// When feature branch is up-to-date with base, returns 0.
#[test]
fn test_commits_behind_returns_zero_when_current() {
    let (dir, root, branch) = make_repo_with_divergence(0);
    let _keep = dir;

    let behind = merges::git::commits_behind(&root, &branch, "main").unwrap();
    assert_eq!(behind, 0, "Branch up-to-date with base should show 0 behind");
}

/// When base has 3 new commits the branch doesn't have, returns 3.
#[test]
fn test_commits_behind_returns_correct_count() {
    let (dir, root, branch) = make_repo_with_divergence(3);
    let _keep = dir;

    let behind = merges::git::commits_behind(&root, &branch, "main").unwrap();
    assert_eq!(behind, 3, "Branch should be 3 commits behind base");
}

/// When base has 1 new commit, returns 1.
#[test]
fn test_commits_behind_returns_one() {
    let (dir, root, branch) = make_repo_with_divergence(1);
    let _keep = dir;

    let behind = merges::git::commits_behind(&root, &branch, "main").unwrap();
    assert_eq!(behind, 1);
}

/// commits_behind returns an error for a non-existent branch.
#[test]
fn test_commits_behind_errors_on_missing_branch() {
    let (dir, root, _branch) = make_repo_with_divergence(0);
    let _keep = dir;

    let result = merges::git::commits_behind(&root, "no-such-branch", "main");
    assert!(result.is_err(), "Should error when branch does not exist");
}

// ── sync_status helper ────────────────────────────────────────────────────────

/// sync_status returns "✓ current" when 0 commits behind.
#[test]
fn test_sync_status_current() {
    let label = merges::git::sync_status(0);
    assert_eq!(label, "✓ current");
}

/// sync_status returns "↓ N behind" when N > 0.
#[test]
fn test_sync_status_behind() {
    let label = merges::git::sync_status(3);
    assert!(label.contains('3'), "Should include count: {}", label);
    assert!(label.contains('↓'), "Should include down-arrow: {}", label);
}
