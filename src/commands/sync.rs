use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

use crate::{git, state::{MergesState, Strategy}};

pub fn run() -> Result<()> {
    let root = git::repo_root()?;
    let state = MergesState::load(&root)?;

    if state.chunks.is_empty() {
        println!("No chunks defined yet.");
        return Ok(());
    }

    let current = git::current_branch(&root)?;

    println!(
        "{} Syncing {} chunk branch(es) onto '{}'",
        "→".blue().bold(),
        state.chunks.len().to_string().yellow(),
        state.base_branch.cyan()
    );

    let pb = ProgressBar::new(state.chunks.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap(),
    );

    for chunk in &state.chunks {
        pb.set_message(format!("rebasing '{}'…", chunk.branch));
        git::checkout(&root, &chunk.branch)?;
        match state.strategy {
            Strategy::Stacked => git::fetch_and_rebase_stacked(&root, &state.base_branch)?,
            Strategy::Independent => git::fetch_and_rebase(&root, &state.base_branch)?,
        }
        pb.inc(1);
    }

    pb.finish_with_message("done");

    // Return to original branch
    git::checkout(&root, &current)?;

    println!("{} All chunks are up to date with '{}'.", "✓".green().bold(), state.base_branch.cyan());
    Ok(())
}
