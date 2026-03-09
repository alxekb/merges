use anyhow::{bail, Result};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Select};

use crate::{git, state::MergesState, ui};

/// Move `files` from `from_chunk` to `to_chunk`.
///
/// When `use_worktrees` is enabled, all operations happen inside each chunk's
/// worktree directory — the main working tree branch never changes.
pub fn run(
    root: &std::path::Path,
    files: &[String],
    from_chunk: &Option<String>,
    to_chunk: &Option<String>,
) -> Result<()> {
    let mut state = MergesState::load(root)?;

    if state.chunks.is_empty() {
        bail!("No chunks defined. Run `merges split` first.");
    }

    let (files_to_move, from_chunk_name, to_chunk_name) = match (files, from_chunk, to_chunk) {
        (f, Some(from), Some(to)) if !f.is_empty() => (f.to_vec(), from.clone(), to.clone()),
        _ => run_interactive(root, &state)?,
    };

    if from_chunk_name == "__unassigned__" {
        // Moving FROM unassigned TO a chunk is equivalent to `merges add`
        if to_chunk_name == "__unassigned__" {
            println!("{} Files are already unassigned.", "·".dimmed());
            return Ok(());
        }
        return crate::commands::add::run(root, &Some(to_chunk_name), &files_to_move);
    }

    // Validate from-chunk
    let from_idx = state
        .chunks
        .iter()
        .position(|c| c.name == from_chunk_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No chunk named '{}'. Available: {}",
                from_chunk_name,
                state.chunks.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
            )
        })?;

    // Validate files are in from-chunk
    for file in &files_to_move {
        if !state.chunks[from_idx].files.contains(file) {
            bail!(
                "File '{}' is not in chunk '{}'. Files in chunk: {}",
                file,
                from_chunk_name,
                state.chunks[from_idx].files.join(", ")
            );
        }
    }

    if to_chunk_name == "__unassigned__" {
        // Moving TO unassigned means just removing it from the chunk
        let source_branch = state.source_branch.clone();
        let from_branch = state.chunks[from_idx].branch.clone();
        let use_worktrees = state.use_worktrees;

        let from_dir = if use_worktrees {
            git::worktree_path(root, &from_branch)
        } else {
            git::checkout(root, &from_branch)?;
            root.to_path_buf()
        };

        remove_files_from_branch(&from_dir, &files_to_move, &source_branch)?;

        if !use_worktrees {
            git::checkout(root, &source_branch)?;
        }

        state.chunks[from_idx].files.retain(|f| !files_to_move.contains(f));
        state.save(root)?;

        println!(
            "{} Removed {} file(s) from chunk '{}' (now unassigned)",
            "✓".green().bold(),
            files_to_move.len().to_string().yellow(),
            from_chunk_name.cyan()
        );
        return Ok(());
    }

    // Validate to-chunk
    let to_idx = state
        .chunks
        .iter()
        .position(|c| c.name == to_chunk_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No chunk named '{}'. Available: {}",
                to_chunk_name,
                state.chunks.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
            )
        })?;

    if from_idx == to_idx {
        println!(
            "{} Files are already in chunk '{}'",
            "·".dimmed(),
            from_chunk_name.cyan()
        );
        return Ok(());
    }

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

    // ── Step 1: Remove files from the from-chunk ──────────────────────────
    remove_files_from_branch(&from_dir, &files_to_move, &source_branch)?;

    // Switch to to-chunk dir
    let to_dir = if use_worktrees {
        git::worktree_path(root, &to_branch)
    } else {
        git::checkout(root, &to_branch)?;
        root.to_path_buf()
    };

    // ── Step 2: Add files to the to-chunk ────────────────────────────────
    let mut files_to_add_to_dest = Vec::new();
    for file in &files_to_move {
        if !state.chunks[to_idx].files.contains(file) {
            files_to_add_to_dest.push(file.clone());
        }
    }

    if !files_to_add_to_dest.is_empty() {
        git::checkout_files_from(&to_dir, &source_branch, &files_to_add_to_dest)?;
        amend_commit(&to_dir, &source_branch)?;
    }

    // ── Step 3: Restore source branch (classic mode only) ─────────────────
    if !use_worktrees {
        git::checkout(root, &source_branch)?;
    }

    // ── Step 4: Update state ──────────────────────────────────────────────
    state.chunks[from_idx].files.retain(|f| !files_to_move.contains(f));
    for file in files_to_move.clone() {
        if !state.chunks[to_idx].files.contains(&file) {
            state.chunks[to_idx].files.push(file);
        }
    }
    state.save(root)?;

    println!(
        "{} Moved {} file(s) from '{}' → '{}'",
        "✓".green().bold(),
        files_to_move.len().to_string().yellow(),
        from_chunk_name.cyan(),
        to_chunk_name.cyan()
    );

    Ok(())
}

fn run_interactive(root: &std::path::Path, state: &MergesState) -> Result<(Vec<String>, String, String)> {
    let assigned: std::collections::HashSet<String> = state.chunks.iter()
        .flat_map(|c| c.files.clone())
        .collect();
    let all_changed = git::changed_files(root, &state.base_branch)?;
    let unassigned: Vec<String> = all_changed.into_iter()
        .filter(|f| !assigned.contains(f))
        .collect();

    // 1. Pick source chunk
    let mut from_options = Vec::new();
    if !unassigned.is_empty() {
        from_options.push(format!("Unassigned files (from {})", state.source_branch).dimmed().to_string());
    }
    for chunk in &state.chunks {
        if !chunk.files.is_empty() {
            from_options.push(format!("{} ({} files)", chunk.name, chunk.files.len()));
        }
    }

    if from_options.is_empty() {
        bail!("All chunks are empty and no unassigned files found.");
    }

    let from_selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Move files FROM:")
        .items(&from_options)
        .default(0)
        .interact()?;

    let (from_chunk_name, files_to_pick_from) = if !unassigned.is_empty() && from_selection == 0 {
        ("__unassigned__".to_string(), unassigned)
    } else {
        // Adjust index if unassigned was present
        let idx = if !unassigned.is_empty() { from_selection - 1 } else { from_selection };
        let active_chunks: Vec<_> = state.chunks.iter().filter(|c| !c.files.is_empty()).collect();
        let chunk = active_chunks[idx];
        (chunk.name.clone(), chunk.files.clone())
    };

    // 2. Pick files
    let selected_files = ui::select_files(
        &format!("Files to move from '{}'", from_chunk_name),
        &files_to_pick_from,
    )?;

    if selected_files.is_empty() {
        bail!("No files selected.");
    }

    // 3. Pick destination chunk
    let mut to_options: Vec<String> = state.chunks.iter().map(|c| c.name.clone()).collect();
    to_options.push("Unassigned (remove from chunks)".dimmed().to_string());

    let to_selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Move {} file(s) TO:", selected_files.len().to_string().yellow()))
        .items(&to_options)
        .default(0)
        .interact()?;

    let to_chunk_name = if to_selection == to_options.len() - 1 {
        "__unassigned__".to_string()
    } else {
        state.chunks[to_selection].name.clone()
    };

    Ok((selected_files, from_chunk_name, to_chunk_name))
}

/// Remove `files` from the tip commit of the branch in `work_dir`.
fn remove_files_from_branch(work_dir: &std::path::Path, files: &[String], source_branch: &str) -> Result<()> {
    let dir = work_dir.to_str().unwrap();

    let status = std::process::Command::new("git")
        .args(["-C", dir, "reset", "--soft", "HEAD~1"])
        .status()?;
    if !status.success() {
        bail!("git reset --soft HEAD~1 failed");
    }

    for file in files {
        // Remove the file from the index and working tree if present. Use --ignore-unmatch
        // so callers can attempt to remove files that may already be absent.
        let status = std::process::Command::new("git")
            .args(["-C", dir, "rm", "-f", "--ignore-unmatch", "--", file])
            .status()?;
        if !status.success() {
            bail!("git rm -f -- {} failed", file);
        }
    }

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
