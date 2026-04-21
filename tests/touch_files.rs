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

    (dir, root)
}

fn write_state(root: &std::path::Path, source_branch: &str) {
    let state = serde_json::json!({
        "base_branch": "main",
        "source_branch": source_branch,
        "repo_owner": "acme",
        "repo_name": "myrepo",
        "strategy": "stacked",
        "use_worktrees": false,
        "chunks": []
    });
    std::fs::write(root.join(".merges.json"), serde_json::to_string_pretty(&state).unwrap()).unwrap();
}

#[test]
fn test_touch_commits_changed_files_and_creates_branch() {
    let (_dir, root) = make_repo();

    // Switch to a feature branch and add uncommitted files
    StdCommand::new("git").args(["checkout", "-b", "feat/touch"]).current_dir(&root).output().unwrap();

    // Create a nested file and another file (untracked/staged)
    std::fs::create_dir_all(root.join("src")) .unwrap();
    std::fs::write(root.join("src").join("new_file.rs"), "// scaffold").unwrap();
    std::fs::write(root.join("other.txt"), "x").unwrap();

    // Stage one file to simulate mixed staged/unstaged state
    StdCommand::new("git").args(["add", "other.txt"]).current_dir(&root).output().unwrap();

    // Write merges state
    write_state(&root, "feat/touch");

    // Prepare plan JSON for the touch command (both files)
    let plan = serde_json::json!([
        {"name": "scaffold", "files": ["src/new_file.rs", "other.txt"]}
    ]);

    // Run non-interactive touch with the plan
    std::env::set_current_dir(&root).unwrap();
    merges::commands::touch::run(Some(plan.to_string())).unwrap();

    // Verify branch was created and files exist (they should be committed on the new branch)
    let state = merges::state::MergesState::load(&root).unwrap();
    assert!(!state.chunks.is_empty(), "state should contain created chunk");
    let branch = &state.chunks[0].branch;

    // Look for most recent commit message on the created branch
    let log = StdCommand::new("git").args(["log", "--oneline", "-1", branch]).current_dir(&root).output().unwrap();
    let log_str = String::from_utf8_lossy(&log.stdout);
    assert!(log_str.contains("chunk"), "Commit message should include 'chunk', got: {}", log_str);

    // Files are committed on the created branch; verify the branch tree contains them
    let tree = StdCommand::new("git").args(["ls-tree", "-r", "--name-only", branch]).current_dir(&root).output().unwrap();
    let tree_str = String::from_utf8_lossy(&tree.stdout);
    assert!(tree_str.contains("src/new_file.rs"), "created branch should contain src/new_file.rs, got: {}", tree_str);
    assert!(tree_str.contains("other.txt"), "created branch should contain other.txt, got: {}", tree_str);
}
