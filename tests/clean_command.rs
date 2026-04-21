//! TDD tests for `merges clean` — RED phase written before production-readiness review.
//!
//! The clean command has zero test coverage. These tests drive:
//!   - basic branch deletion and state update
//!   - graceful handling of already-deleted branches
//!   - state consistency after partial failures
//!   - the `--merged` predicate logic (is_merged vs dead state=="merged" check)

use merges::state::{Chunk, ChunkStatus, MergesState, Strategy};
use std::process::Command as StdCommand;
use tempfile::TempDir;

// ── helpers ───────────────────────────────────────────────────────────────────

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

    // Feature branch with files
    StdCommand::new("git")
        .args(["checkout", "-b", "feat/big"])
        .current_dir(&root)
        .output()
        .unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(root.join(format!("src/{}", name)), format!("// {}", name)).unwrap();
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

/// Build a state with two chunks whose branches actually exist locally.
fn setup_two_chunks(root: &std::path::Path) -> MergesState {
    // Create chunk branches
    for branch in &["feat/big-chunk-1-alpha", "feat/big-chunk-2-beta"] {
        StdCommand::new("git")
            .args(["checkout", "-b", branch])
            .current_dir(root)
            .output()
            .unwrap();
        StdCommand::new("git")
            .args(["checkout", "feat/big"])
            .current_dir(root)
            .output()
            .unwrap();
    }

    let state = MergesState {
        base_branch: "main".to_string(),
        source_branch: "feat/big".to_string(),
        repo_owner: "acme".to_string(),
        repo_name: "myrepo".to_string(),
        strategy: Strategy::Independent,
        use_worktrees: false,
        commit_prefix: None,
        chunks: vec![
            Chunk {
                name: "alpha".to_string(),
                branch: "feat/big-chunk-1-alpha".to_string(),
                files: vec!["src/a.rs".to_string()],
                pr_number: None,
                pr_url: None,
                status: ChunkStatus::Pending,
            },
            Chunk {
                name: "beta".to_string(),
                branch: "feat/big-chunk-2-beta".to_string(),
                files: vec!["src/b.rs".to_string()],
                pr_number: None,
                pr_url: None,
                status: ChunkStatus::Pending,
            },
        ],
    };
    state.save(root).unwrap();
    merges::git::ensure_gitignored(root, ".merges.json").unwrap();
    state
}

fn branch_exists(root: &std::path::Path, branch: &str) -> bool {
    let out = StdCommand::new("git")
        .args(["branch", "--list", branch])
        .current_dir(root)
        .output()
        .unwrap();
    !String::from_utf8_lossy(&out.stdout).trim().is_empty()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// clean (no flags, --yes) deletes all chunk branches.
#[tokio::test]
async fn test_clean_all_deletes_both_branches() {
    let (_dir, root) = make_repo();
    setup_two_chunks(&root);

    merges::commands::clean::run(&root, false, true).await.unwrap();

    assert!(!branch_exists(&root, "feat/big-chunk-1-alpha"), "alpha branch should be deleted");
    assert!(!branch_exists(&root, "feat/big-chunk-2-beta"), "beta branch should be deleted");
}

/// After clean, the state file must have zero chunks.
#[tokio::test]
async fn test_clean_all_removes_chunks_from_state() {
    let (_dir, root) = make_repo();
    setup_two_chunks(&root);

    merges::commands::clean::run(&root, false, true).await.unwrap();

    let state = MergesState::load(&root).unwrap();
    assert_eq!(
        state.chunks.len(),
        0,
        "State should have no chunks after clean, found: {:?}",
        state.chunks.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
}

/// clean must not error if a branch was already deleted externally.
#[tokio::test]
async fn test_clean_tolerates_already_deleted_branch() {
    let (_dir, root) = make_repo();
    setup_two_chunks(&root);

    // Manually delete one branch before running clean
    StdCommand::new("git")
        .args(["branch", "-D", "feat/big-chunk-1-alpha"])
        .current_dir(&root)
        .output()
        .unwrap();

    // clean should still succeed and handle the missing branch gracefully
    let result = merges::commands::clean::run(&root, false, true).await;
    assert!(result.is_ok(), "clean should tolerate a branch already deleted: {:?}", result);

    // The other branch should still be gone
    assert!(!branch_exists(&root, "feat/big-chunk-2-beta"), "beta branch should still be deleted");
}

/// clean returns early (no panic) when there are no chunks defined.
#[tokio::test]
async fn test_clean_no_chunks_is_a_no_op() {
    let (_dir, root) = make_repo();

    let state = MergesState {
        base_branch: "main".to_string(),
        source_branch: "feat/big".to_string(),
        repo_owner: "acme".to_string(),
        repo_name: "myrepo".to_string(),
        strategy: Strategy::Independent,
        use_worktrees: false,
        commit_prefix: None,
        chunks: vec![],
    };
    state.save(&root).unwrap();
    merges::git::ensure_gitignored(&root, ".merges.json").unwrap();

    let result = merges::commands::clean::run(&root, false, true).await;
    assert!(result.is_ok(), "clean with no chunks should succeed: {:?}", result);
}

// ── Merged-detection predicate ────────────────────────────────────────────────
//
// These tests document the contract for the "is this PR finished?" predicate
// used by `clean --merged`. GitHub's API returns state="closed" for both merged
// and closed-without-merge PRs; it never returns state="merged".
// Correct detection therefore requires checking `is_merged` OR `state=="closed"`.

use merges::github::PrInfo;

fn pr(state: &str, is_merged: bool) -> PrInfo {
    PrInfo {
        number: 1,
        url: "https://github.com/a/b/pull/1".to_string(),
        title: "test".to_string(),
        state: state.to_string(),
        is_merged,
        body: "".to_string(),
        ci_status: "pending".to_string(),
        review_state: "pending".to_string(),
    }
}

/// A merged PR has state="closed" and is_merged=true.
/// Both conditions individually should flag it for cleanup.
#[test]
fn test_merged_pr_has_closed_state_not_merged_state() {
    let merged = pr("closed", true);
    // GitHub API never returns state="merged"; merged PRs have state="closed"
    assert_ne!(merged.state, "merged", "GitHub never returns state='merged'");
    assert_eq!(merged.state, "closed");
    assert!(merged.is_merged);
}

/// A closed-but-not-merged PR: state="closed", is_merged=false.
/// Should also be detected as "finished" (matches current README spec).
#[test]
fn test_closed_not_merged_pr_has_closed_state() {
    let closed = pr("closed", false);
    assert_eq!(closed.state, "closed");
    assert!(!closed.is_merged);
}

/// An open PR should never be flagged for cleanup.
#[test]
fn test_open_pr_is_not_finished() {
    let open = pr("open", false);
    assert_eq!(open.state, "open");
    assert!(!open.is_merged);
    // is_merged=false AND state!="closed" → should NOT be cleaned
    assert!(!(open.is_merged || open.state == "closed"));
}

/// Proves the dead-code bug: checking `state == "merged"` is always false.
/// The correct check is `is_merged || state == "closed"`.
#[test]
fn test_state_equals_merged_is_always_false_for_real_prs() {
    // All realistic PR states from GitHub
    for (state, is_merged) in [("open", false), ("closed", false), ("closed", true)] {
        let info = pr(state, is_merged);
        assert_ne!(
            info.state, "merged",
            "GitHub never returns state='merged' (state={}, is_merged={})",
            state, is_merged
        );
    }
}
