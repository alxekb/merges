use anyhow::{bail, Result};
use serde_json;

use crate::{git, split::ChunkPlan, state::{Chunk, MergesState}};

/// Non-interactive touch command: create branches/worktrees for chunks and
/// touch the listed files (create empty files/directories) then commit them.
pub fn run(plan_json: Option<String>) -> Result<()> {
    let root = git::repo_root()?;
    let mut state = MergesState::load(&root)?;

    let json = plan_json.ok_or_else(|| anyhow::anyhow!("Provide --plan JSON for `merges touch`"))?;
    let plan: Vec<ChunkPlan> = serde_json::from_str(&json)
        .map_err(|e| anyhow::anyhow!("Invalid --plan JSON: {}", e))?;

    if plan.is_empty() {
        bail!("Chunk plan is empty — provide at least one chunk with files.");
    }

    // Validate files are in the diff vs base
    let changed = git::changed_files(&root, &state.base_branch)?;
    for chunk in &plan {
        for file in &chunk.files {
            if !changed.contains(file) {
                bail!("File '{}' in chunk '{}' is not in the diff between '{}' and HEAD.", file, chunk.name, state.base_branch);
            }
        }
    }

    // Ensure no duplicates with existing state
    let already_assigned: Vec<&str> = state.chunks.iter()
        .flat_map(|c| c.files.iter().map(|f| f.as_str()))
        .collect();
    for chunk in &plan {
        for file in &chunk.files {
            if already_assigned.contains(&file.as_str()) {
                bail!("File '{}' is already assigned to an existing chunk. Use `merges move` to reassign it.", file);
            }
        }
    }

    // No file duplicated within the plan
    let mut seen = std::collections::HashSet::new();
    for chunk in &plan {
        for file in &chunk.files {
            if !seen.insert(file.as_str()) {
                bail!("File '{}' appears more than once across the chunk plan.", file);
            }
        }
    }

    let base_sha = git::merge_base(&root, &state.base_branch)?;
    let use_worktrees = state.use_worktrees;

    // Build effective prefix for commit messages
    let effective_prefix = state.commit_prefix.clone().or_else(|| git::ticket_prefix(&state.source_branch)).unwrap_or_default();

    let source_branch = state.source_branch.clone();
    let mut created_branches: Vec<String> = Vec::new();

    let result = (|| -> Result<Vec<Chunk>> {
        let mut new_chunks = Vec::new();
        for (i, chunk_plan) in plan.iter().enumerate() {
            let n = state.chunks.len() + new_chunks.len() + 1;
            let safe_name = chunk_plan.name.to_lowercase().replace(' ', "-");
            let branch = format!("{}-chunk-{}-{}", source_branch, n, safe_name);

            let work_dir: std::path::PathBuf = if use_worktrees {
                git::add_worktree(&root, &branch, &base_sha)?;
                git::worktree_path(&root, &branch)
            } else {
                git::create_branch(&root, &branch, &base_sha)?;
                root.to_path_buf()
            };
            created_branches.push(branch.clone());

            // Touch files in work_dir
            for file in &chunk_plan.files {
                let dest = work_dir.join(file);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                if !dest.exists() {
                    std::fs::write(&dest, "")?;
                }
            }

            let body = format!("chunk {} - {}\n\nFiles:\n{}", n, chunk_plan.name, chunk_plan.files.join("\n"));
            // Use same commit formatting as other commands so hooks remain compatible
            let msg = git::commit_message(&source_branch, &body);

            git::commit_all(&work_dir, &msg)?;

            if !use_worktrees {
                // Return to source branch so subsequent branch creations are safe
                git::checkout(&root, &source_branch)?;
            }

            new_chunks.push(Chunk {
                name: chunk_plan.name.clone(),
                branch: branch.clone(),
                files: chunk_plan.files.clone(),
                pr_number: None,
                pr_url: None,
                status: crate::state::ChunkStatus::Pending,
            });
        }
        Ok(new_chunks)
    })();

    match result {
        Ok(new_chunks) => {
            state.chunks.extend(new_chunks);
            state.save(&root)?;
            Ok(())
        }
        Err(e) => {
            // Rollback: clean up any branches/worktrees we created.
            if !use_worktrees {
                let _ = git::checkout(&root, &source_branch);
            }
            for branch in &created_branches {
                if use_worktrees {
                    let _ = git::remove_worktree(&root, branch);
                }
                let _ = git::delete_branch(&root, branch);
            }
            Err(e)
        }
    }
}
