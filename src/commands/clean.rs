use anyhow::Result;
use colored::Colorize;
use dialoguer::Confirm;

use crate::{config, git, github, state::MergesState};

pub async fn run(merged_only: bool, yes: bool) -> Result<()> {
    let root = git::repo_root()?;
    let mut state = MergesState::load(&root)?;

    if state.chunks.is_empty() {
        println!("No chunks defined.");
        return Ok(());
    }

    // Optionally check GitHub to find merged PRs
    let merged_pr_numbers: Vec<u64> = if merged_only {
        let token = config::github_token().ok();
        if let Some(tok) = token {
            let gh = github::client(&tok)?;
            let mut merged = vec![];
            for chunk in &state.chunks {
                let Some(pr_num) = chunk.pr_number else { continue };
                let Ok(info) = github::get_pr_info(&gh, &state.repo_owner, &state.repo_name, pr_num).await else { continue };
                if info.state == "closed" || info.state == "merged" {
                    merged.push(pr_num);
                }
            }
            merged
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    // Determine which chunks to clean
    let to_clean: Vec<usize> = state
        .chunks
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            if merged_only {
                c.pr_number.map(|n| merged_pr_numbers.contains(&n)).unwrap_or(false)
            } else {
                true
            }
        })
        .map(|(i, _)| i)
        .collect();

    if to_clean.is_empty() {
        println!(
            "{}",
            if merged_only {
                "No merged chunks found to clean.".to_string()
            } else {
                "No chunks to clean.".to_string()
            }
        );
        return Ok(());
    }

    println!(
        "{} {} chunk branch(es) will be deleted:",
        "→".blue().bold(),
        to_clean.len().to_string().yellow()
    );
    for &i in &to_clean {
        println!("  • {}", state.chunks[i].branch.cyan());
    }

    if !yes {
        let confirmed = Confirm::new()
            .with_prompt("Delete these branches?")
            .default(false)
            .interact()?;
        if !confirmed {
            println!("Aborted.");
            return Ok(());
        }
    }

    let current = git::current_branch(&root)?;

    // Delete in reverse order so indices remain valid
    let mut removed_branches = vec![];
    for &i in to_clean.iter().rev() {
        let branch = &state.chunks[i].branch;

        // Switch away if we're on this branch
        if current == *branch {
            git::checkout(&root, &state.base_branch)?;
        }

        match git::delete_branch(&root, branch) {
            Ok(_) => {
                // Also remove worktree if worktrees mode is enabled
                if state.use_worktrees {
                    let _ = git::remove_worktree(&root, branch);
                }
                println!("{} Deleted local branch '{}'", "✓".green(), branch.cyan());
                removed_branches.push(branch.clone());
            }
            Err(e) => {
                println!("{} Failed to delete '{}': {}", "!".yellow(), branch.cyan(), e);
            }
        }
    }

    // Remove cleaned chunks from state
    state
        .chunks
        .retain(|c| !removed_branches.contains(&c.branch));
    state.save(&root)?;

    println!(
        "\n{} Cleaned {} chunk(s). {} chunk(s) remain.",
        "✓".green().bold(),
        removed_branches.len().to_string().yellow(),
        state.chunks.len().to_string().yellow()
    );

    Ok(())
}
