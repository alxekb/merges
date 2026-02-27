use anyhow::{bail, Result};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Select};

use crate::{git, state::MergesState, ui};

/// Add `files` to the named chunk.
///
/// When `use_worktrees` is enabled, operations happen inside the chunk's
/// worktree directory — the main working tree branch never changes.
/// In classic mode, the chunk branch is checked out and then restored.
pub fn run(root: &std::path::Path, chunk_name: &Option<String>, files: &[String]) -> Result<()> {
    let mut state = MergesState::load(root)?;

    if state.chunks.is_empty() {
        bail!("No chunks defined. Run `merges split` first.");
    }

    let (target_chunk_name, files_to_add) = match (chunk_name, files) {
        (Some(c), f) if !f.is_empty() => (c.clone(), f.to_vec()),
        _ => run_interactive(root, &state)?,
    };

    // Find the chunk
    let chunk_idx = state
        .chunks
        .iter()
        .position(|c| c.name == target_chunk_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No chunk named '{}'. Available chunks: {}",
                target_chunk_name,
                state.chunks.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
            )
        })?;

    // Validate all files are in the diff
    let changed = git::changed_files(root, &state.base_branch)?;
    for file in &files_to_add {
        if !changed.contains(file) {
            bail!(
                "File '{}' is not in the diff between '{}' and HEAD.",
                file,
                state.base_branch
            );
        }
    }

    // Deduplicate: only add files not already in the chunk
    let existing = &state.chunks[chunk_idx].files;
    let new_files: Vec<String> = files_to_add
        .iter()
        .filter(|f| !existing.contains(f))
        .cloned()
        .collect();

    let source_branch = state.source_branch.clone();
    let chunk_branch = state.chunks[chunk_idx].branch.clone();

    if new_files.is_empty() {
        println!(
            "{} All specified files are already in chunk '{}' — nothing to do.",
            "·".dimmed(),
            target_chunk_name.cyan()
        );
        return Ok(());
    }

    // Determine the working directory for this chunk
    let work_dir = if state.use_worktrees {
        git::worktree_path(root, &chunk_branch)
    } else {
        // Classic mode: switch to chunk branch, amend, restore
        git::checkout(root, &chunk_branch)?;
        root.to_path_buf()
    };

    let result = (|| -> Result<()> {
        git::checkout_files_from(&work_dir, &source_branch, &new_files)?;

        let amend_status = std::process::Command::new("git")
            .args(["-C", work_dir.to_str().unwrap(), "add", "-A"])
            .status()?;
        if !amend_status.success() {
            bail!("git add failed");
        }

        let amend_status = std::process::Command::new("git")
            .args(["-C", work_dir.to_str().unwrap(), "commit", "--amend", "--no-edit"])
            .status()?;
        if !amend_status.success() {
            bail!("git commit --amend failed");
        }

        Ok(())
    })();

    // Classic mode: always restore source branch
    if !state.use_worktrees {
        git::checkout(root, &source_branch)?;
    }

    result?;

    println!(
        "{} Added {} file(s) to chunk '{}'",
        "✓".green().bold(),
        new_files.len().to_string().yellow(),
        target_chunk_name.cyan()
    );

    // Update state
    state.chunks[chunk_idx].files.extend(new_files);
    state.save(root)?;

    Ok(())
}

fn run_interactive(root: &std::path::Path, state: &MergesState) -> Result<(String, Vec<String>)> {
    if state.chunks.is_empty() {
        bail!("No chunks defined. Run `merges split` first.");
    }

    // 1. Pick target chunk
    let chunk_idx = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Add files TO chunk:")
        .items(&state.chunks.iter().map(|c| format!("{} ({} files)", c.name, c.files.len())).collect::<Vec<_>>())
        .default(0)
        .interact()?;

    let target_chunk = &state.chunks[chunk_idx];

    // 2. Pick unassigned files
    let assigned: std::collections::HashSet<String> = state.chunks.iter()
        .flat_map(|c| c.files.clone())
        .collect();
    let all_changed = git::changed_files(root, &state.base_branch)?;
    let unassigned: Vec<String> = all_changed.into_iter()
        .filter(|f| !assigned.contains(f))
        .collect();

    if unassigned.is_empty() {
        bail!("No unassigned files found on branch '{}'.", state.source_branch);
    }

    let selected_files = ui::select_files(
        &format!("Select files to add to '{}'", target_chunk.name),
        &unassigned,
    )?;

    if selected_files.is_empty() {
        bail!("No files selected.");
    }

    Ok((target_chunk.name.clone(), selected_files))
}
