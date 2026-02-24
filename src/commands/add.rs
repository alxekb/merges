use anyhow::{bail, Result};
use colored::Colorize;

use crate::{git, state::MergesState};

/// Add `files` to the named chunk.
///
/// Steps:
/// 1. Validate files exist in the source-branch diff.
/// 2. Checkout the chunk branch.
/// 3. Checkout new files from the source branch.
/// 4. Amend the chunk commit to include them.
/// 5. Return to the source branch.
/// 6. Update the state file.
pub fn run(root: &std::path::Path, chunk_name: &str, files: &[String]) -> Result<()> {
    let mut state = MergesState::load(root)?;

    // Find the chunk
    let chunk_idx = state
        .chunks
        .iter()
        .position(|c| c.name == chunk_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No chunk named '{}'. Available chunks: {}",
                chunk_name,
                state.chunks.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
            )
        })?;

    if files.is_empty() {
        bail!("No files provided.");
    }

    // Validate all files are in the diff
    let changed = git::changed_files(root, &state.base_branch)?;
    for file in files {
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
    let new_files: Vec<String> = files
        .iter()
        .filter(|f| !existing.contains(f))
        .cloned()
        .collect();

    let source_branch = state.source_branch.clone();
    let chunk_branch = state.chunks[chunk_idx].branch.clone();

    // Switch to chunk branch, add the new files, amend commit
    git::checkout(root, &chunk_branch)?;

    if !new_files.is_empty() {
        git::checkout_files_from(root, &source_branch, &new_files)?;

        // Amend the existing commit
        let amend_status = std::process::Command::new("git")
            .args(["-C", root.to_str().unwrap(), "add", "-A"])
            .status()?;
        if !amend_status.success() {
            git::checkout(root, &source_branch)?;
            bail!("git add failed");
        }

        let amend_status = std::process::Command::new("git")
            .args([
                "-C",
                root.to_str().unwrap(),
                "commit",
                "--amend",
                "--no-edit",
            ])
            .status()?;
        if !amend_status.success() {
            git::checkout(root, &source_branch)?;
            bail!("git commit --amend failed");
        }

        println!(
            "{} Added {} file(s) to chunk '{}'",
            "✓".green().bold(),
            new_files.len().to_string().yellow(),
            chunk_name.cyan()
        );
    } else {
        println!(
            "{} All specified files are already in chunk '{}' — nothing to do.",
            "·".dimmed(),
            chunk_name.cyan()
        );
    }

    git::checkout(root, &source_branch)?;

    // Update state: add new files to chunk
    state.chunks[chunk_idx].files.extend(new_files);
    state.save(root)?;

    Ok(())
}
