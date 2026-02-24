use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

use crate::{git, state::MergesState};

/// Result of a doctor run: a list of human-readable issues found.
#[derive(Debug)]
pub struct DoctorReport {
    pub issues: Vec<String>,
}

impl DoctorReport {
    pub fn all_ok(&self) -> bool {
        self.issues.is_empty()
    }
}

/// Validate state consistency. If `repair` is true, attempt to fix issues in place.
pub fn run(root: &Path, repair: bool) -> Result<DoctorReport> {
    let state = MergesState::load(root)?;
    let mut issues = Vec::new();

    // 1. Check each chunk branch exists locally
    for chunk in &state.chunks {
        let out = std::process::Command::new("git")
            .args(["branch", "--list", &chunk.branch])
            .current_dir(root)
            .output()?;
        let output = String::from_utf8_lossy(&out.stdout);
        if output.trim().is_empty() {
            issues.push(format!("Chunk branch '{}' does not exist locally.", chunk.branch));
        }
    }

    // 2. Check worktrees exist when use_worktrees is enabled
    if state.use_worktrees {
        for chunk in &state.chunks {
            let wt = git::worktree_path(root, &chunk.branch);
            if !wt.exists() {
                issues.push(format!(
                    "Worktree for branch '{}' missing at '{}'.",
                    chunk.branch,
                    wt.display()
                ));
            }
        }
    }

    // 3. Check .merges.json is in .git/info/exclude
    let exclude_path = root.join(".git/info/exclude");
    let exclude_content = std::fs::read_to_string(&exclude_path).unwrap_or_default();
    if !exclude_content.contains(".merges.json") {
        issues.push(".merges.json is not in .git/info/exclude — it may appear as an untracked file.".to_string());
        if repair {
            git::ensure_gitignored(root, ".merges.json")?;
            issues.pop(); // resolved
        }
    }

    // 4. Check no file appears in multiple chunks
    let mut seen: HashSet<&str> = HashSet::new();
    for chunk in &state.chunks {
        for file in &chunk.files {
            if !seen.insert(file.as_str()) {
                issues.push(format!(
                    "File '{}' appears in multiple chunks (duplicate in state — possibly corrupted).",
                    file
                ));
            }
        }
    }

    Ok(DoctorReport { issues })
}
