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
        "{} Syncing {} chunk branch(es) onto '{}'{}",
        "→".blue().bold(),
        state.chunks.len().to_string().yellow(),
        state.base_branch.cyan(),
        if state.use_worktrees { " (parallel)" } else { "" }
    );

    let pb = ProgressBar::new(state.chunks.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap(),
    );

    if state.use_worktrees {
        // Parallel rebase: each chunk has its own worktree dir — no serialization needed
        use std::sync::{Arc, Mutex};
        let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        let pb = Arc::new(pb);

        std::thread::scope(|s| {
            for chunk in &state.chunks {
                let wt = git::worktree_path(&root, &chunk.branch);
                let base = state.base_branch.clone();
                let strategy = state.strategy.clone();
                let name = chunk.branch.clone();
                let pb = Arc::clone(&pb);
                let errors = Arc::clone(&errors);

                s.spawn(move || {
                    let result = match strategy {
                        Strategy::Stacked => git::fetch_and_rebase_stacked(&wt, &base),
                        Strategy::Independent => git::fetch_and_rebase(&wt, &base),
                    };
                    if let Err(e) = result {
                        errors.lock().unwrap().push(format!("{}: {}", name, e));
                    }
                    pb.inc(1);
                });
            }
        });

        pb.finish_with_message("done");

        let errs = errors.lock().unwrap();
        if !errs.is_empty() {
            anyhow::bail!("Some chunks failed to rebase:\n{}", errs.join("\n"));
        }
    } else {
        // Classic mode: sequential, requires branch checkout
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
        git::checkout(&root, &current)?;
    }

    println!("{} All chunks are up to date with '{}'.", "✓".green().bold(), state.base_branch.cyan());
    Ok(())
}
