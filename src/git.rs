use anyhow::{bail, Context, Result};
use git2::Repository;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Find the git repository root from the current directory.
pub fn repo_root() -> Result<PathBuf> {
    let repo = Repository::discover(".")
        .context("Not inside a git repository")?;
    let workdir = repo.workdir()
        .context("Bare repositories are not supported")?;
    Ok(workdir.to_path_buf())
}

/// Return the name of the currently checked-out branch.
pub fn current_branch(root: &Path) -> Result<String> {
    let repo = Repository::open(root)?;
    let head = repo.head().context("No HEAD — is this a fresh repo?")?;
    head.shorthand()
        .map(|s| s.to_string())
        .context("HEAD is detached or has no name")
}

/// List files changed between `base_branch` and HEAD (working-tree aware).
pub fn changed_files(root: &Path, base_branch: &str) -> Result<Vec<String>> {
    // Use git diff --name-only for reliability across merge-base scenarios.
    let output = Command::new("git")
        .args([
            "-C",
            root.to_str().unwrap(),
            "diff",
            "--name-only",
            &format!("{}...HEAD", base_branch),
        ])
        .output()
        .context("Failed to run `git diff`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git diff failed: {}", stderr);
    }

    let files = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.to_string())
        .filter(|l| !l.is_empty())
        .collect();

    Ok(files)
}

/// Create a new branch pointing at `base_ref` (e.g. the merge-base with main).
pub fn create_branch(root: &Path, branch_name: &str, base_ref: &str) -> Result<()> {
    let status = Command::new("git")
        .args([
            "-C",
            root.to_str().unwrap(),
            "checkout",
            "-b",
            branch_name,
            base_ref,
        ])
        .status()
        .context("Failed to run `git checkout -b`")?;

    if !status.success() {
        bail!("Failed to create branch '{}'", branch_name);
    }
    Ok(())
}

/// Checkout an existing branch.
pub fn checkout(root: &Path, branch_name: &str) -> Result<()> {
    let status = Command::new("git")
        .args(["-C", root.to_str().unwrap(), "checkout", branch_name])
        .status()
        .context("Failed to run `git checkout`")?;

    if !status.success() {
        bail!("Failed to checkout branch '{}'", branch_name);
    }
    Ok(())
}

/// Find the merge-base commit between `base_branch` and HEAD.
pub fn merge_base(root: &Path, base_branch: &str) -> Result<String> {
    let output = Command::new("git")
        .args([
            "-C",
            root.to_str().unwrap(),
            "merge-base",
            base_branch,
            "HEAD",
        ])
        .output()
        .context("Failed to run `git merge-base`")?;

    if !output.status.success() {
        bail!("git merge-base failed");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Cherry-pick (copy) specific files from `source_branch` into the current branch
/// by checking out those files from `source_branch` and committing.
pub fn checkout_files_from(root: &Path, source_branch: &str, files: &[String]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }

    let mut args = vec![
        "-C".to_string(),
        root.to_str().unwrap().to_string(),
        "checkout".to_string(),
        source_branch.to_string(),
        "--".to_string(),
    ];
    args.extend(files.iter().cloned());

    let status = Command::new("git")
        .args(&args)
        .status()
        .context("Failed to checkout files from source branch")?;

    if !status.success() {
        bail!("Failed to checkout files from '{}'", source_branch);
    }
    Ok(())
}

/// Stage all files and create a commit.
pub fn commit_all(root: &Path, message: &str) -> Result<()> {
    let add_out = Command::new("git")
        .args(["-C", root.to_str().unwrap(), "add", "-A"])
        .output()?;
    if !add_out.status.success() {
        bail!("git add failed: {}", String::from_utf8_lossy(&add_out.stderr).trim());
    }

    let commit_out = Command::new("git")
        .args(["-C", root.to_str().unwrap(), "commit", "-m", message])
        .output()?;
    if !commit_out.status.success() {
        let stderr = String::from_utf8_lossy(&commit_out.stderr);
        let stdout = String::from_utf8_lossy(&commit_out.stdout);
        // git prints "nothing to commit" on stdout, not stderr
        let detail = if stdout.contains("nothing to commit") || stderr.contains("nothing to commit") {
            "nothing to commit, working tree clean".to_string()
        } else {
            format!("{}{}", stderr.trim(), stdout.trim())
        };
        bail!("git commit failed: {}", detail);
    }
    Ok(())
}

/// Fetch latest origin and rebase current branch onto `base_branch`.
pub fn fetch_and_rebase(root: &Path, base_branch: &str) -> Result<()> {
    fetch(root)?;
    rebase(root, base_branch, false)
}

/// Like `fetch_and_rebase` but passes `--update-refs` so stacked chunk branches
/// that point at commits in the rebased history are automatically updated.
pub fn fetch_and_rebase_stacked(root: &Path, base_branch: &str) -> Result<()> {
    fetch(root)?;
    rebase(root, base_branch, true)
}

fn fetch(root: &Path) -> Result<()> {
    let status = Command::new("git")
        .args(["-C", root.to_str().unwrap(), "fetch", "origin"])
        .status()
        .context("git fetch failed")?;
    if !status.success() {
        bail!("git fetch origin failed");
    }
    Ok(())
}

fn rebase(root: &Path, base_branch: &str, update_refs: bool) -> Result<()> {
    let mut args = vec![
        "-C".to_string(),
        root.to_str().unwrap().to_string(),
        "rebase".to_string(),
    ];
    if update_refs {
        args.push("--update-refs".to_string());
    }
    args.push(format!("origin/{}", base_branch));

    let status = Command::new("git")
        .args(&args)
        .status()
        .context("git rebase failed")?;
    if !status.success() {
        bail!(
            "Rebase onto origin/{} failed — resolve conflicts then run `merges sync` again",
            base_branch
        );
    }
    Ok(())
}

/// Push a branch to origin (force-with-lease to handle rebases safely).
pub fn push_branch(root: &Path, branch_name: &str) -> Result<()> {
    let status = Command::new("git")
        .args([
            "-C",
            root.to_str().unwrap(),
            "push",
            "origin",
            branch_name,
            "--force-with-lease",
        ])
        .status()
        .context("git push failed")?;
    if !status.success() {
        bail!("Failed to push branch '{}'", branch_name);
    }
    Ok(())
}

/// Delete a local branch (must not be currently checked out).
pub fn delete_branch(root: &Path, branch_name: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["-C", root.to_str().unwrap(), "branch", "-D", branch_name])
        .output()
        .context("Failed to run `git branch -D`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git branch -D failed: {}", stderr.trim());
    }
    Ok(())
}

// ── Worktree helpers ──────────────────────────────────────────────────────────

/// Return the path where a worktree for `branch_name` will be created.
/// Located at `.git/merges-worktrees/<sanitised-branch>` — inside `.git/`
/// so it is never tracked or shown in `git status`.
pub fn worktree_path(root: &Path, branch_name: &str) -> PathBuf {
    let safe = branch_name.replace('/', "-");
    root.join(".git").join("merges-worktrees").join(safe)
}

/// Create a new branch `branch_name` at `base_ref` and add a worktree for it.
/// The main worktree (and current branch) is untouched.
pub fn add_worktree(root: &Path, branch_name: &str, base_ref: &str) -> Result<()> {
    let wt_path = worktree_path(root, branch_name);
    std::fs::create_dir_all(wt_path.parent().unwrap())?;

    let status = Command::new("git")
        .args([
            "-C",
            root.to_str().unwrap(),
            "worktree",
            "add",
            "-b",
            branch_name,
            wt_path.to_str().unwrap(),
            base_ref,
        ])
        .status()
        .context("git worktree add failed")?;

    if !status.success() {
        bail!("Failed to create worktree for branch '{}'", branch_name);
    }
    Ok(())
}

/// Remove the worktree for `branch_name` and delete the directory.
pub fn remove_worktree(root: &Path, branch_name: &str) -> Result<()> {
    let wt_path = worktree_path(root, branch_name);
    if !wt_path.exists() {
        return Ok(());
    }

    let status = Command::new("git")
        .args([
            "-C",
            root.to_str().unwrap(),
            "worktree",
            "remove",
            "--force",
            wt_path.to_str().unwrap(),
        ])
        .status()
        .context("git worktree remove failed")?;

    if !status.success() {
        bail!("Failed to remove worktree for branch '{}'", branch_name);
    }
    Ok(())
}

/// Ensure `pattern` appears in `.git/info/exclude` (local gitignore, never committed).
/// This keeps `.merges.json` from appearing in diffs or blocking branch checkouts,
/// without polluting the project's `.gitignore`.
pub fn ensure_gitignored(root: &Path, pattern: &str) -> Result<()> {
    let git_dir = root.join(".git");
    let info_dir = git_dir.join("info");
    std::fs::create_dir_all(&info_dir)?;
    let exclude = info_dir.join("exclude");

    let existing = if exclude.exists() {
        std::fs::read_to_string(&exclude)?
    } else {
        String::new()
    };

    if existing.lines().any(|l| l.trim() == pattern) {
        return Ok(());
    }

    let entry = if existing.is_empty() || existing.ends_with('\n') {
        format!("{}\n", pattern)
    } else {
        format!("\n{}\n", pattern)
    };

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&exclude)?;
    file.write_all(entry.as_bytes())?;

    Ok(())
}

/// Enable `rerere` for this repository so conflict resolutions are recorded
/// and automatically replayed. Equivalent to:
///   git config rerere.enabled true
///   git config rerere.autoupdate true
pub fn enable_rerere(root: &Path) -> Result<()> {
    for (key, val) in [("rerere.enabled", "true"), ("rerere.autoupdate", "true")] {
        let status = Command::new("git")
            .args(["-C", root.to_str().unwrap(), "config", key, val])
            .status()
            .context("Failed to run `git config`")?;
        if !status.success() {
            bail!("git config {} {} failed", key, val);
        }
    }
    Ok(())
}

/// Parse `owner/repo` from `git remote get-url origin`.
pub fn remote_owner_repo(root: &Path) -> Result<(String, String)> {
    let output = Command::new("git")
        .args(["-C", root.to_str().unwrap(), "remote", "get-url", "origin"])
        .output()
        .context("Failed to get remote URL")?;

    if !output.status.success() {
        bail!("No 'origin' remote found");
    }

    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_github_owner_repo(&url)
}

pub(crate) fn parse_github_owner_repo(url: &str) -> Result<(String, String)> {
    // Handles both https://github.com/owner/repo.git and git@github.com:owner/repo.git
    // Trim surrounding whitespace first so shell output with trailing newlines works.
    let stripped = url
        .trim()
        .trim_end_matches(".git")
        .trim_end_matches('/');

    if let Some(path) = stripped.strip_prefix("git@github.com:") {
        let parts: Vec<&str> = path.splitn(2, '/').collect();
        if parts.len() == 2 {
            return Ok((parts[0].to_string(), parts[1].to_string()));
        }
    }

    if let Some(rest) = stripped.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.splitn(2, '/').collect();
        if parts.len() == 2 {
            return Ok((parts[0].to_string(), parts[1].to_string()));
        }
    }

    bail!("Cannot parse GitHub owner/repo from remote URL: {}", url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;
    use tempfile::TempDir;

    // ── helpers ──────────────────────────────────────────────────────────────

    /// Create a temporary git repo with one commit and return (TempDir, root path).
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

    // ── parse_github_owner_repo ────────────────────────────────────────────

    #[test]
    fn test_parse_https_with_git_suffix() {
        let (owner, repo) = parse_github_owner_repo("https://github.com/acme/myrepo.git").unwrap();
        assert_eq!(owner, "acme");
        assert_eq!(repo, "myrepo");
    }

    #[test]
    fn test_parse_https_without_git_suffix() {
        let (owner, repo) = parse_github_owner_repo("https://github.com/acme/myrepo").unwrap();
        assert_eq!(owner, "acme");
        assert_eq!(repo, "myrepo");
    }

    #[test]
    fn test_parse_https_with_trailing_slash() {
        let (owner, repo) = parse_github_owner_repo("https://github.com/acme/myrepo/").unwrap();
        assert_eq!(owner, "acme");
        assert_eq!(repo, "myrepo");
    }

    #[test]
    fn test_parse_ssh_with_git_suffix() {
        let (owner, repo) = parse_github_owner_repo("git@github.com:acme/myrepo.git").unwrap();
        assert_eq!(owner, "acme");
        assert_eq!(repo, "myrepo");
    }

    #[test]
    fn test_parse_ssh_without_git_suffix() {
        let (owner, repo) = parse_github_owner_repo("git@github.com:acme/myrepo").unwrap();
        assert_eq!(owner, "acme");
        assert_eq!(repo, "myrepo");
    }

    #[test]
    fn test_parse_url_with_hyphens_and_dots_in_names() {
        let (owner, repo) = parse_github_owner_repo("https://github.com/my-org/my.repo_name.git").unwrap();
        assert_eq!(owner, "my-org");
        assert_eq!(repo, "my.repo_name");
    }

    /// ❌ RED: `git remote get-url` output often has a trailing newline.
    /// parse_github_owner_repo must strip leading/trailing whitespace before parsing.
    #[test]
    fn test_parse_url_with_trailing_newline() {
        let (owner, repo) = parse_github_owner_repo("https://github.com/acme/myrepo.git\n").unwrap();
        assert_eq!(owner, "acme");
        assert_eq!(repo, "myrepo");
    }

    /// ❌ RED: SSH URL with trailing newline (common in shell output).
    #[test]
    fn test_parse_ssh_url_with_trailing_newline() {
        let (owner, repo) = parse_github_owner_repo("git@github.com:acme/myrepo.git\n").unwrap();
        assert_eq!(owner, "acme");
        assert_eq!(repo, "myrepo");
    }

    #[test]
    fn test_parse_gitlab_url_returns_error() {
        let result = parse_github_owner_repo("https://gitlab.com/acme/myrepo.git");
        assert!(result.is_err(), "Non-GitHub URLs should be rejected");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Cannot parse"), "Error message should explain what failed: {}", msg);
    }

    #[test]
    fn test_parse_empty_url_returns_error() {
        let result = parse_github_owner_repo("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_owner_only_url_returns_error() {
        let result = parse_github_owner_repo("https://github.com/acme");
        assert!(result.is_err(), "URL without repo should be rejected");
    }

    // ── current_branch ────────────────────────────────────────────────────

    #[test]
    fn test_current_branch_returns_branch_name() {
        let (_dir, root) = make_repo();
        let branch = current_branch(&root).unwrap();
        assert_eq!(branch, "main");
    }

    #[test]
    fn test_current_branch_after_checkout() {
        let (_dir, root) = make_repo();
        StdCommand::new("git").args(["checkout", "-b", "feat/test"]).current_dir(&root).output().unwrap();
        let branch = current_branch(&root).unwrap();
        assert_eq!(branch, "feat/test");
    }

    // ── changed_files ─────────────────────────────────────────────────────

    #[test]
    fn test_changed_files_empty_when_no_changes() {
        let (_dir, root) = make_repo();
        let files = changed_files(&root, "main").unwrap();
        assert!(files.is_empty(), "No changes vs HEAD should return empty list");
    }

    #[test]
    fn test_changed_files_detects_new_file_on_branch() {
        let (_dir, root) = make_repo();

        // Create a feature branch with a new file
        StdCommand::new("git").args(["checkout", "-b", "feat/test"]).current_dir(&root).output().unwrap();
        std::fs::write(root.join("new_file.rs"), "fn foo() {}").unwrap();
        StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
        StdCommand::new("git").args(["commit", "-m", "add new_file"]).current_dir(&root).output().unwrap();

        let files = changed_files(&root, "main").unwrap();
        assert_eq!(files, vec!["new_file.rs"]);
    }

    #[test]
    fn test_changed_files_detects_multiple_files() {
        let (_dir, root) = make_repo();
        StdCommand::new("git").args(["checkout", "-b", "feat/multi"]).current_dir(&root).output().unwrap();

        for name in ["a.rs", "b.rs", "c.rs"] {
            std::fs::write(root.join(name), "").unwrap();
        }
        StdCommand::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
        StdCommand::new("git").args(["commit", "-m", "add files"]).current_dir(&root).output().unwrap();

        let mut files = changed_files(&root, "main").unwrap();
        files.sort();
        assert_eq!(files, vec!["a.rs", "b.rs", "c.rs"]);
    }

    // ── commit_all ────────────────────────────────────────────────────────

    /// Committing with nothing staged should return a descriptive error mentioning
    /// "nothing to commit" so engineers understand what went wrong.
    #[test]
    fn test_commit_all_with_nothing_staged_returns_error() {
        let (_dir, root) = make_repo();
        // git add -A on a clean tree is fine but git commit returns non-zero
        let result = commit_all(&root, "empty commit");
        assert!(result.is_err(), "Committing with nothing new should fail");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("nothing to commit"),
            "Error should explain why commit failed, got: {}",
            msg
        );
    }

    #[test]
    fn test_commit_all_stages_and_commits_new_file() {
        let (_dir, root) = make_repo();
        std::fs::write(root.join("new.txt"), "content").unwrap();
        commit_all(&root, "add new.txt").unwrap();

        // Verify the commit exists
        let log = StdCommand::new("git").args(["log", "--oneline", "-1"]).current_dir(&root).output().unwrap();
        let log_str = String::from_utf8_lossy(&log.stdout);
        assert!(log_str.contains("add new.txt"));
    }

    // ── create_branch / checkout ──────────────────────────────────────────

    #[test]
    fn test_create_branch_creates_and_checks_out_branch() {
        let (_dir, root) = make_repo();
        create_branch(&root, "feat/new", "HEAD").unwrap();
        let branch = current_branch(&root).unwrap();
        assert_eq!(branch, "feat/new");
    }

    #[test]
    fn test_create_branch_duplicate_name_returns_error() {
        let (_dir, root) = make_repo();
        create_branch(&root, "feat/dup", "HEAD").unwrap();
        // Switch back and try to create same branch again
        checkout(&root, "main").unwrap();
        let result = create_branch(&root, "feat/dup", "HEAD");
        assert!(result.is_err());
    }

    #[test]
    fn test_checkout_switches_branch() {
        let (_dir, root) = make_repo();
        create_branch(&root, "feat/switch", "HEAD").unwrap();
        checkout(&root, "main").unwrap();
        let branch = current_branch(&root).unwrap();
        assert_eq!(branch, "main");
    }

    #[test]
    fn test_checkout_nonexistent_branch_returns_error() {
        let (_dir, root) = make_repo();
        let result = checkout(&root, "branch-does-not-exist");
        assert!(result.is_err());
    }

    // ── delete_branch ─────────────────────────────────────────────────────

    #[test]
    fn test_delete_branch_removes_local_branch() {
        let (_dir, root) = make_repo();
        create_branch(&root, "feat/to-delete", "HEAD").unwrap();
        checkout(&root, "main").unwrap();
        delete_branch(&root, "feat/to-delete").unwrap();

        let branches = StdCommand::new("git")
            .args(["branch", "--list", "feat/to-delete"])
            .current_dir(&root)
            .output()
            .unwrap();
        let out = String::from_utf8_lossy(&branches.stdout);
        assert!(out.trim().is_empty(), "Branch should be gone after deletion");
    }

    #[test]
    fn test_delete_nonexistent_branch_returns_error() {
        let (_dir, root) = make_repo();
        let result = delete_branch(&root, "does-not-exist");
        assert!(result.is_err());
    }

    // ── ensure_gitignored ─────────────────────────────────────────────────

    #[test]
    fn test_ensure_gitignored_creates_exclude_file() {
        let (_dir, root) = make_repo();
        ensure_gitignored(&root, ".merges.json").unwrap();
        let exclude = root.join(".git/info/exclude");
        assert!(exclude.exists());
        let content = std::fs::read_to_string(&exclude).unwrap();
        assert!(content.contains(".merges.json"));
    }

    #[test]
    fn test_ensure_gitignored_idempotent() {
        let (_dir, root) = make_repo();
        ensure_gitignored(&root, ".merges.json").unwrap();
        ensure_gitignored(&root, ".merges.json").unwrap(); // second call should not duplicate
        let exclude = root.join(".git/info/exclude");
        let content = std::fs::read_to_string(&exclude).unwrap();
        let count = content.lines().filter(|l| l.trim() == ".merges.json").count();
        assert_eq!(count, 1, "Pattern should appear exactly once, found {}", count);
    }

    #[test]
    fn test_ensure_gitignored_appends_to_existing_file() {
        let (_dir, root) = make_repo();
        let info = root.join(".git/info");
        std::fs::create_dir_all(&info).unwrap();
        std::fs::write(info.join("exclude"), "# existing rules\n*.log\n").unwrap();

        ensure_gitignored(&root, ".merges.json").unwrap();

        let content = std::fs::read_to_string(info.join("exclude")).unwrap();
        assert!(content.contains("*.log"), "Existing rules should be preserved");
        assert!(content.contains(".merges.json"), "New pattern should be appended");
    }
}
