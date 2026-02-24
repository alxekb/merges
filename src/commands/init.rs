use anyhow::{bail, Result};
use colored::Colorize;
use dialoguer::{Confirm, Input};

use crate::{
    git,
    state::{MergesState, Strategy},
};

pub fn run(base_branch: Option<String>) -> Result<()> {
    let root = git::repo_root()?;
    let state_path = crate::state::MergesState::path(&root);

    if state_path.exists() {
        let overwrite = Confirm::new()
            .with_prompt(".merges.json already exists — overwrite?")
            .default(false)
            .interact()?;
        if !overwrite {
            bail!("Aborted.");
        }
    }

    let source_branch = git::current_branch(&root)?;

    let base: String = if let Some(b) = base_branch {
        b
    } else {
        Input::new()
            .with_prompt("Base branch (target for PRs)")
            .default("main".to_string())
            .interact_text()?
    };

    let (owner, repo) = git::remote_owner_repo(&root)?;

    let state = MergesState {
        base_branch: base.clone(),
        source_branch: source_branch.clone(),
        repo_owner: owner.clone(),
        repo_name: repo.clone(),
        strategy: Strategy::Stacked, // default; overridden by `push --independent`
        chunks: vec![],
    };

    state.save(&root)?;
    git::ensure_gitignored(&root, ".merges.json")?;
    git::enable_rerere(&root)?;

    println!(
        "{} Initialised merges for {}/{} — source: {}, base: {}",
        "✓".green().bold(),
        owner.cyan(),
        repo.cyan(),
        source_branch.yellow(),
        base.yellow()
    );
    println!("  {} rerere enabled — conflict resolutions will be replayed automatically.", "·".dimmed());
    println!(
        "  Next: run {} to assign files to chunks.",
        "merges split".bold()
    );

    Ok(())
}
