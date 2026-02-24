//! Non-interactive chunk splitting logic.
//! Used by both the TUI command and the MCP tool.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{
    git,
    state::{Chunk, MergesState},
};

/// Describes one chunk in a plan: a name and the files it should contain.
/// This is the serialisable struct consumed by `apply_plan` and the MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkPlan {
    pub name: String,
    pub files: Vec<String>,
}

/// Automatically group `files` into chunks by directory structure.
///
/// Strategy:
/// - If all changed files live under a **single** top-level directory (e.g. all
///   under `src/`), group by the **second** path component so that
///   `src/models/*` and `src/api/*` become separate `"models"` and `"api"` chunks.
/// - If files are spread across **multiple** top-level directories (e.g. `frontend/`
///   and `backend/`), group by the first path component.
/// - Files at the repository root (no directory) go into a chunk named `"root"`.
///
/// Returns one `ChunkPlan` per unique key, sorted alphabetically, with files
/// within each chunk also sorted. Returns an empty vec when `files` is empty.
///
/// This is a pure function with no git or filesystem side-effects — easy to test.
pub fn auto_group_files(files: &[String]) -> Vec<ChunkPlan> {
    use std::collections::HashSet;

    if files.is_empty() {
        return vec![];
    }

    // Compute the top-level directory (or "root") for each file
    let top_dirs: HashSet<String> = files.iter().map(|f| top_dir(f)).collect();
    let non_root_tops: Vec<&String> = top_dirs.iter().filter(|d| d.as_str() != "root").collect();

    // If there is exactly one non-root top-level dir, look one level deeper
    let use_second_level = non_root_tops.len() == 1;

    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in files {
        let key = grouping_key(file, use_second_level);
        groups.entry(key).or_default().push(file.clone());
    }

    groups
        .into_iter()
        .map(|(name, mut files)| {
            files.sort();
            ChunkPlan { name, files }
        })
        .collect()
}

/// Return the first path component, or "root" for files with no parent directory.
fn top_dir(file: &str) -> String {
    let path = std::path::Path::new(file);
    match path.components().next() {
        Some(std::path::Component::Normal(s)) if path.parent() != Some(std::path::Path::new("")) => {
            s.to_str().unwrap_or("root").to_string()
        }
        _ => "root".to_string(),
    }
}

/// Choose a grouping key for a file.
///
/// - 1-component path (`file.rs`) → "root"
/// - 2-component path (`dir/file.rs`) → always `dir`
/// - 3+ component path (`dir/sub/file.rs`):
///   - `use_second_level=true`  → `sub`
///   - `use_second_level=false` → `dir`
fn grouping_key(file: &str, use_second_level: bool) -> String {
    let components: Vec<&str> = std::path::Path::new(file)
        .components()
        .filter_map(|c| {
            if let std::path::Component::Normal(s) = c {
                s.to_str()
            } else {
                None
            }
        })
        .collect();

    match components.as_slice() {
        [] | [_] => "root".to_string(),
        [dir, _file] => dir.to_string(),
        [dir, subdir, ..] => {
            if use_second_level {
                subdir.to_string()
            } else {
                dir.to_string()
            }
        }
    }
}

/// Apply a pre-built chunk plan to the repository atomically:
/// 1. Validates that all files in the plan are actually in the diff vs base.
/// 2. For each chunk, creates a branch from the merge-base, cherry-picks files, commits.
/// 3. Returns to the original source branch.
/// 4. Saves chunk definitions to the state file.
///
/// If any step fails, ALL previously created chunk branches are deleted and the
/// state file is left unchanged (atomic all-or-nothing semantics).
///
/// This is the testable core of `merges split`, used by both the interactive TUI
/// and the MCP `merges_split` tool.
pub fn apply_plan(root: &std::path::Path, plan: Vec<ChunkPlan>) -> Result<()> {
    if plan.is_empty() {
        bail!("Chunk plan is empty — provide at least one chunk with files.");
    }

    let mut state = MergesState::load(root)?;
    let source_branch = state.source_branch.clone();
    let base_branch = state.base_branch.clone();

    // Ensure .merges.json won't block branch checkouts (it must be gitignored)
    git::ensure_gitignored(root, ".merges.json")?;

    // Validate ALL files upfront before touching any branches
    let changed = git::changed_files(root, &base_branch)?;
    for chunk in &plan {
        for file in &chunk.files {
            if !changed.contains(file) {
                bail!(
                    "File '{}' in chunk '{}' is not in the diff between '{}' and HEAD. \
                     Changed files are: {:?}",
                    file,
                    chunk.name,
                    base_branch,
                    changed
                );
            }
        }
    }

    let base_sha = git::merge_base(root, &base_branch)?;
    let use_worktrees = state.use_worktrees;

    // Track branches we create so we can roll them back on failure.
    let mut created_branches: Vec<String> = Vec::new();

    let result = (|| -> Result<Vec<Chunk>> {
        let mut new_chunks = Vec::new();
        for chunk_plan in &plan {
            let n = state.chunks.len() + new_chunks.len() + 1;
            let safe_name = chunk_plan.name.to_lowercase().replace(' ', "-");
            let branch = format!("{}-chunk-{}-{}", source_branch, n, safe_name);

            let work_dir: std::path::PathBuf = if use_worktrees {
                git::add_worktree(root, &branch, &base_sha)?;
                git::worktree_path(root, &branch)
            } else {
                git::create_branch(root, &branch, &base_sha)?;
                root.to_path_buf()
            };
            created_branches.push(branch.clone());

            git::checkout_files_from(&work_dir, &source_branch, &chunk_plan.files)?;

            let msg = format!(
                "feat({}): chunk {} - {}\n\nFiles:\n{}",
                safe_name,
                n,
                chunk_plan.name,
                chunk_plan.files.join("\n")
            );
            git::commit_all(&work_dir, &msg)?;

            // Classic mode: return to source branch after each chunk
            if !use_worktrees {
                git::checkout(root, &source_branch)?;
            }

            new_chunks.push(Chunk {
                name: chunk_plan.name.clone(),
                branch,
                files: chunk_plan.files.clone(),
                pr_number: None,
                pr_url: None,
            });
        }
        Ok(new_chunks)
    })();

    match result {
        Ok(new_chunks) => {
            state.chunks.extend(new_chunks);
            state.save(root)?;
            Ok(())
        }
        Err(e) => {
            // Rollback: clean up any branches/worktrees we created.
            if !use_worktrees {
                let _ = git::checkout(root, &source_branch);
            }
            for branch in &created_branches {
                if use_worktrees {
                    let _ = git::remove_worktree(root, branch);
                }
                let _ = git::delete_branch(root, branch);
            }
            Err(e)
        }
    }
}
