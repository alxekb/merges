//! TDD tests for `merges doctor` — RED phase.

use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

fn make_repo_with_state() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();

    StdCommand::new("git").args(["init", "-b", "main"]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["config", "user.name", "T"]).current_dir(&root).output().unwrap();
    fs::write(root.join("README.md"), "hello").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "init"]).current_dir(&root).output().unwrap();

    // Create feature branch with a file
    StdCommand::new("git").args(["checkout", "-b", "feat/big"]).current_dir(&root).output().unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "fn main() {}").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "add src"]).current_dir(&root).output().unwrap();

    (dir, root)
}

fn write_state_with_chunk(root: &std::path::Path, chunk_branch: &str) {
    let state = serde_json::json!({
        "base_branch": "main",
        "source_branch": "feat/big",
        "repo_owner": "acme",
        "repo_name": "myrepo",
        "strategy": "independent",
        "use_worktrees": false,
        "chunks": [{
            "name": "models",
            "branch": chunk_branch,
            "files": ["src/lib.rs"],
            "pr_number": null,
            "pr_url": null,
            "status": "pending"
        }]
    });
    fs::write(root.join(".merges.json"), serde_json::to_string_pretty(&state).unwrap()).unwrap();
}

// ── Doctor checks ─────────────────────────────────────────────────────────────

/// doctor returns Ok and all-clear when state and branches are consistent.
#[test]
fn test_doctor_healthy_state_returns_ok() {
    let (_dir, root) = make_repo_with_state();

    // Create the chunk branch so state is consistent
    StdCommand::new("git")
        .args(["checkout", "-b", "feat/big-chunk-models"])
        .current_dir(&root)
        .output().unwrap();
    StdCommand::new("git")
        .args(["checkout", "feat/big"])
        .current_dir(&root)
        .output().unwrap();

    write_state_with_chunk(&root, "feat/big-chunk-models");
    // Ensure .merges.json is properly excluded so doctor sees a clean state
    merges::git::ensure_gitignored(&root, ".merges.json").unwrap();

    let report = merges::doctor::run(&root, false).unwrap();
    assert!(report.all_ok(), "Healthy state should report all checks ok: {:?}", report);
}

/// doctor detects missing chunk branch.
#[test]
fn test_doctor_detects_missing_branch() {
    let (_dir, root) = make_repo_with_state();
    write_state_with_chunk(&root, "feat/big-chunk-models");
    // Do NOT create the branch — it's missing

    let report = merges::doctor::run(&root, false).unwrap();
    assert!(!report.all_ok(), "Should detect missing branch");
    assert!(
        report.issues.iter().any(|i| i.contains("feat/big-chunk-models")),
        "Issue should name the missing branch: {:?}", report.issues
    );
}

/// doctor detects .merges.json not in .git/info/exclude.
#[test]
fn test_doctor_detects_missing_gitignore_entry() {
    let (_dir, root) = make_repo_with_state();

    StdCommand::new("git")
        .args(["checkout", "-b", "feat/big-chunk-models"])
        .current_dir(&root).output().unwrap();
    StdCommand::new("git")
        .args(["checkout", "feat/big"])
        .current_dir(&root).output().unwrap();

    write_state_with_chunk(&root, "feat/big-chunk-models");
    // Do NOT add .merges.json to .git/info/exclude

    let report = merges::doctor::run(&root, false).unwrap();
    let has_gitignore_issue = report.issues.iter().any(|i| i.contains(".merges.json") || i.contains("exclude"));
    assert!(has_gitignore_issue, "Should detect missing gitignore entry: {:?}", report.issues);
}

/// doctor --repair re-adds .merges.json to .git/info/exclude.
#[test]
fn test_doctor_repair_restores_gitignore_entry() {
    let (_dir, root) = make_repo_with_state();

    StdCommand::new("git")
        .args(["checkout", "-b", "feat/big-chunk-models"])
        .current_dir(&root).output().unwrap();
    StdCommand::new("git")
        .args(["checkout", "feat/big"])
        .current_dir(&root).output().unwrap();

    write_state_with_chunk(&root, "feat/big-chunk-models");

    // Run with repair
    merges::doctor::run(&root, true).unwrap();

    let exclude = fs::read_to_string(root.join(".git/info/exclude")).unwrap_or_default();
    assert!(exclude.contains(".merges.json"), "Repair should add .merges.json to .git/info/exclude");
}

/// doctor detects duplicate file across chunks in state.
#[test]
fn test_doctor_detects_duplicate_files_in_state() {
    let (_dir, root) = make_repo_with_state();

    // Write state with two chunks sharing the same file (corrupted state)
    let state = serde_json::json!({
        "base_branch": "main",
        "source_branch": "feat/big",
        "repo_owner": "acme",
        "repo_name": "myrepo",
        "strategy": "independent",
        "use_worktrees": false,
        "chunks": [
            {
                "name": "a",
                "branch": "feat/big-chunk-a",
                "files": ["src/lib.rs"],
                "pr_number": null, "pr_url": null, "status": "pending"
            },
            {
                "name": "b",
                "branch": "feat/big-chunk-b",
                "files": ["src/lib.rs"],
                "pr_number": null, "pr_url": null, "status": "pending"
            }
        ]
    });
    fs::write(root.join(".merges.json"), serde_json::to_string_pretty(&state).unwrap()).unwrap();
    // Create branches so only the file-dup check fires
    for branch in &["feat/big-chunk-a", "feat/big-chunk-b"] {
        StdCommand::new("git").args(["checkout", "-b", branch]).current_dir(&root).output().unwrap();
        StdCommand::new("git").args(["checkout", "feat/big"]).current_dir(&root).output().unwrap();
    }
    // Add gitignore entry to isolate check
    merges::git::ensure_gitignored(&root, ".merges.json").unwrap();

    let report = merges::doctor::run(&root, false).unwrap();
    let has_dup = report.issues.iter().any(|i| i.contains("src/lib.rs") || i.contains("duplicate"));
    assert!(has_dup, "Should detect duplicate file across chunks: {:?}", report.issues);
}
