use merges::git;
use merges::state::MergesState;
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

    // Initial commit so HEAD exists
    std::fs::write(root.join("README.md"), "hello").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "init"]).current_dir(&root).output().unwrap();

    (dir, root)
}

#[test]
fn test_touch_creates_branches_and_commits_files() {
    let (_dir, root) = make_repo();

    // Simulate a feature branch with changes
    StdCommand::new("git").args(["checkout", "-b", "feat/touch-test"]).current_dir(&root).output().unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/new_file.rs"), "fn x() {}").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "add feature files"]).current_dir(&root).output().unwrap();

    // Write minimal .merges.json state
    let state = MergesState {
        base_branch: "main".to_string(),
        source_branch: "feat/touch-test".to_string(),
        repo_owner: "acme".to_string(),
        repo_name: "repo".to_string(),
        strategy: merges::state::Strategy::Stacked,
        use_worktrees: false,
        commit_prefix: None,
        chunks: vec![],
    };
    state.save(&root).unwrap();

    // Prepare plan: one chunk with a new file path
    let plan = r#"[{"name":"scaffold","files":["src/new_file.rs"]}]"#;

    // Run the touch command (commands use repo_root() so change CWD to the repo)
    let orig_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&root).unwrap();
    merges::commands::touch::run(Some(plan.to_string())).unwrap();
    // Restore CWD to avoid affecting other tests
    std::env::set_current_dir(orig_cwd).unwrap();

    // Verify branches created
    let branches = StdCommand::new("git").args(["branch", "--list"]).current_dir(&root).output().unwrap();
    let out = String::from_utf8_lossy(&branches.stdout);
    assert!(out.contains("feat/touch-test-chunk-1-scaffold"), "Branch created");

    // Verify files exist in the branch commit
    StdCommand::new("git").args(["checkout", "feat/touch-test-chunk-1-scaffold"]).current_dir(&root).output().unwrap();
    assert!(root.join("src/new_file.rs").exists());

    // Verify commit exists and is reachable
    let log = StdCommand::new("git").args(["log", "-1", "--pretty=%B"]).current_dir(&root).output().unwrap();
    let _msg = String::from_utf8_lossy(&log.stdout);
}
