use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

fn make_repo_with_origin() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();

    StdCommand::new("git").args(["init", "-b", "main"]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["config", "user.name", "T"]).current_dir(&root).output().unwrap();

    fs::write(root.join("README.md"), "root content").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "init"]).current_dir(&root).output().unwrap();

    // Add origin pointing to self so fetch/rebase works in tests
    StdCommand::new("git").args(["remote", "add", "origin", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["fetch", "origin"]).current_dir(&root).output().unwrap();

    // Feature branch
    StdCommand::new("git").args(["checkout", "-b", "feat/big"]).current_dir(&root).output().unwrap();
    fs::write(root.join("feature.txt"), "feature content").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "feature work"]).current_dir(&root).output().unwrap();

    // Initialize merges state
    let state = serde_json::json!({
        "base_branch": "main",
        "source_branch": "feat/big",
        "repo_owner": "acme",
        "repo_name": "myrepo",
        "strategy": "independent",
        "use_worktrees": false,
        "chunks": [
            {
                "name": "chunk1",
                "branch": "feat/big-chunk1",
                "files": ["feature.txt"],
                "status": "pending"
            }
        ]
    });
    fs::write(root.join(".merges.json"), serde_json::to_string_pretty(&state).unwrap()).unwrap();
    merges::git::ensure_gitignored(&root, ".merges.json").unwrap();

    // Create the chunk branch
    StdCommand::new("git").args(["checkout", "-b", "feat/big-chunk1"]).current_dir(&root).output().unwrap();
    
    (dir, root)
}

#[test]
fn test_sync_checkouts_to_source_branch_even_if_started_from_chunk() {
    let (_dir, root) = make_repo_with_origin();
    std::env::set_current_dir(&root).unwrap();

    // Currently we are on feat/big-chunk1
    let initial_branch = merges::git::current_branch(&root).unwrap();
    assert_eq!(initial_branch, "feat/big-chunk1");

    // Run sync
    merges::commands::sync::run().expect("sync should succeed");

    // Verify we are on feat/big (the source_branch), NOT feat/big-chunk1
    let final_branch = merges::git::current_branch(&root).unwrap();
    assert_eq!(final_branch, "feat/big", "Should have checked out to the root (source) branch after sync");
}
