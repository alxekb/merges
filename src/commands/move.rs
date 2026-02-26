use anyhow::{bail, Result};
use colored::Colorize;

use crate::{git, state::MergesState};

/// Move `file` from `from_chunk` to `to_chunk`.
///
/// When `use_worktrees` is enabled, all operations happen inside each chunk's
/// worktree directory — the main working tree branch never changes.
pub fn run(
    root: &std::path::Path,
    file: &str,
    from_chunk: &str,
    to_chunk: &str,
) -> Result<()> {
    let mut state = MergesState::load(root)?;

    // Validate from-chunk
    let from_idx = state
        .chunks
        .iter()
        .position(|c| c.name == from_chunk)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No chunk named '{}'. Available: {}",
                from_chunk,
                state.chunks.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
            )
        })?;

    // Validate file is in from-chunk
    if !state.chunks[from_idx].files.contains(&file.to_string()) {
        bail!(
            "File '{}' is not in chunk '{}'. Files in chunk: {}",
            file,
            from_chunk,
            state.chunks[from_idx].files.join(", ")
        );
    }

    // Validate to-chunk
    let to_idx = state
        .chunks
        .iter()
        .position(|c| c.name == to_chunk)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No chunk named '{}'. Available: {}",
                to_chunk,
                state.chunks.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
            )
        })?;

    let source_branch = state.source_branch.clone();
    let from_branch = state.chunks[from_idx].branch.clone();
    let to_branch = state.chunks[to_idx].branch.clone();
    let use_worktrees = state.use_worktrees;

    // Resolve the working directories for each chunk
    let from_dir = if use_worktrees {
        git::worktree_path(root, &from_branch)
    } else {
        git::checkout(root, &from_branch)?;
        root.to_path_buf()
    };

    // ── Step 1: Remove file from the from-chunk ───────────────────────────
    remove_file_from_branch(&from_dir, file, &source_branch)?;

    // Switch to to-chunk dir
    let to_dir = if use_worktrees {
        git::worktree_path(root, &to_branch)
    } else {
        git::checkout(root, &to_branch)?;
        root.to_path_buf()
    };

    // ── Step 2: Add file to the to-chunk ─────────────────────────────────
    if !state.chunks[to_idx].files.contains(&file.to_string()) {
        git::checkout_files_from(&to_dir, &source_branch, &[file.to_string()])?;
        amend_commit(&to_dir, &source_branch)?;
    }

    // ── Step 3: Restore source branch (classic mode only) ─────────────────
    if !use_worktrees {
        git::checkout(root, &source_branch)?;
    }

    // ── Step 4: Update state ──────────────────────────────────────────────
    state.chunks[from_idx].files.retain(|f| f != file);
    if !state.chunks[to_idx].files.contains(&file.to_string()) {
        state.chunks[to_idx].files.push(file.to_string());
    }
    state.save(root)?;

    println!(
        "{} Moved '{}' from '{}' → '{}'",
        "✓".green().bold(),
        file.yellow(),
        from_chunk.cyan(),
        to_chunk.cyan()
    );

    Ok(())
}

/// Remove `file` from the tip commit of the branch in `work_dir`.
fn remove_file_from_branch(work_dir: &std::path::Path, file: &str, source_branch: &str) -> Result<()> {
    let dir = work_dir.to_str().unwrap();

    let status = std::process::Command::new("git")
        .args(["-C", dir, "reset", "--soft", "HEAD~1"])
        .status()?;
    if !status.success() {
        bail!("git reset --soft HEAD~1 failed");
    }

    let status = std::process::Command::new("git")
        .args(["-C", dir, "reset", "HEAD", "--", file])
        .status()?;
    if !status.success() {
        bail!("git reset HEAD -- {} failed", file);
    }

    let _ = std::process::Command::new("git")
        .args(["-C", dir, "checkout", "--", file])
        .status();

    let out = std::process::Command::new("git")
        .args(["-C", dir, "diff", "--cached", "--name-only"])
        .output()?;
    let staged = String::from_utf8_lossy(&out.stdout);

    if staged.trim().is_empty() {
        let msg = crate::git::commit_message(source_branch, "chunk: (empty after move)");
        let status = std::process::Command::new("git")
            .args(["-C", dir, "commit", "--allow-empty", "-m", &msg])
            .status()?;
        if !status.success() {
            bail!("git commit --allow-empty failed");
        }
    } else {
        let msg = crate::git::commit_message(source_branch, "chunk: update files");
        let status = std::process::Command::new("git")
            .args(["-C", dir, "commit", "--no-edit", "-m", &msg])
            .status()?;
        if !status.success() {
            bail!("git commit failed after removing file");
        }
    }

    Ok(())
}

/// Stage everything and amend the tip commit in `work_dir`.
fn amend_commit(work_dir: &std::path::Path, _source_branch: &str) -> Result<()> {
    let dir = work_dir.to_str().unwrap();

    let status = std::process::Command::new("git")
        .args(["-C", dir, "add", "-A"])
        .status()?;
    if !status.success() {
        bail!("git add failed");
    }

    let status = std::process::Command::new("git")
        .args(["-C", dir, "commit", "--amend", "--no-edit"])
        .status()?;
    if !status.success() {
        bail!("git commit --amend failed");
    }

    Ok(())
}
