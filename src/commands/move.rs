use anyhow::{bail, Result};
use colored::Colorize;

use crate::{git, state::MergesState};

/// Move `file` from `from_chunk` to `to_chunk`.
///
/// Steps:
/// 1. Validate both chunks exist and `file` is in `from_chunk`.
/// 2. Remove `file` from the `from_chunk` branch (checkout prev commit, amend).
/// 3. Add `file` to the `to_chunk` branch (checkout from source, amend).
/// 4. Update state file.
/// 5. Restore source branch.
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

    // ── Step 1: Remove file from the from-chunk branch ────────────────────
    git::checkout(root, &from_branch)?;
    remove_file_from_branch(root, file, &source_branch)?;

    // ── Step 2: Add file to the to-chunk branch ───────────────────────────
    git::checkout(root, &to_branch)?;
    if !state.chunks[to_idx].files.contains(&file.to_string()) {
        git::checkout_files_from(root, &source_branch, &[file.to_string()])?;
        amend_commit(root, &source_branch)?;
    }

    // ── Step 3: Update state ──────────────────────────────────────────────
    state.chunks[from_idx].files.retain(|f| f != file);
    if !state.chunks[to_idx].files.contains(&file.to_string()) {
        state.chunks[to_idx].files.push(file.to_string());
    }

    // Restore source branch before saving (save reads root state from CWD)
    git::checkout(root, &source_branch)?;
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

/// Remove `file` from the tip commit of the currently checked-out branch.
/// Strategy: soft-reset, unstage the file, commit the rest.
fn remove_file_from_branch(root: &std::path::Path, file: &str, source_branch: &str) -> Result<()> {
    let root_str = root.to_str().unwrap();

    // Soft-reset to parent — un-commits everything but keeps working tree
    let status = std::process::Command::new("git")
        .args(["-C", root_str, "reset", "--soft", "HEAD~1"])
        .status()?;
    if !status.success() {
        git::checkout(root, source_branch)?;
        bail!("git reset --soft HEAD~1 failed");
    }

    // Unstage (reset) the file we want to remove
    let status = std::process::Command::new("git")
        .args(["-C", root_str, "reset", "HEAD", "--", file])
        .status()?;
    if !status.success() {
        git::checkout(root, source_branch)?;
        bail!("git reset HEAD -- {} failed", file);
    }

    // Restore the file in the working tree to its pre-commit state (discard it)
    let _ = std::process::Command::new("git")
        .args(["-C", root_str, "checkout", "--", file])
        .status();

    // Check if anything remains staged
    let out = std::process::Command::new("git")
        .args(["-C", root_str, "diff", "--cached", "--name-only"])
        .output()?;
    let staged = String::from_utf8_lossy(&out.stdout);

    if staged.trim().is_empty() {
        // Nothing left — create an empty commit to keep branch valid
        // Actually for chunk branches we allow empty commits to mark the split point
        let status = std::process::Command::new("git")
            .args(["-C", root_str, "commit", "--allow-empty", "-m", "chunk: (empty after move)"])
            .status()?;
        if !status.success() {
            git::checkout(root, source_branch)?;
            bail!("git commit --allow-empty failed");
        }
    } else {
        let status = std::process::Command::new("git")
            .args(["-C", root_str, "commit", "--no-edit", "-m", "chunk: update files"])
            .status()?;
        if !status.success() {
            git::checkout(root, source_branch)?;
            bail!("git commit failed after removing file");
        }
    }

    Ok(())
}

/// Stage everything and amend the tip commit on the current branch.
fn amend_commit(root: &std::path::Path, source_branch: &str) -> Result<()> {
    let root_str = root.to_str().unwrap();

    let status = std::process::Command::new("git")
        .args(["-C", root_str, "add", "-A"])
        .status()?;
    if !status.success() {
        git::checkout(root, source_branch)?;
        bail!("git add failed");
    }

    let status = std::process::Command::new("git")
        .args(["-C", root_str, "commit", "--amend", "--no-edit"])
        .status()?;
    if !status.success() {
        git::checkout(root, source_branch)?;
        bail!("git commit --amend failed");
    }

    Ok(())
}
