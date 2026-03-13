use anyhow::{bail, Result};
use colored::Colorize;
use crate::{git, split::ChunkPlan, state::MergesState};

/// Touch command: create branches/worktrees for chunks and touch the listed files
/// (create empty files/directories) then commit them. Useful for scaffold-only PRs.
pub fn run(plan_json: Option<String>) -> Result<()> {
    let root = git::repo_root()?;
    let state = MergesState::load(&root)?;

    let plan = if let Some(json) = plan_json {
        // Non-interactive path: parse plan from JSON
        let plan: Vec<ChunkPlan> = serde_json::from_str(&json)
            .map_err(|e| anyhow::anyhow!("Invalid --plan JSON: {}", e))?;
        plan
    } else {
        // Interactive mode: allow user to pick files
        use crate::ui;
        use dialoguer::Input;

        let all_files = git::changed_files(&root, &state.base_branch)?;
        if all_files.is_empty() {
            bail!("No changed files found between HEAD and '{}'", state.base_branch);
        }

        println!("{} Found {} changed file(s)", "→".blue().bold(), all_files.len());

        let selected = ui::select_files("Select files to touch", &all_files)?;
        if selected.is_empty() {
            println!("{} No files selected — nothing to do.", "!".yellow().bold());
            return Ok(());
        }

        let chunk_name: String = Input::new()
            .with_prompt("Chunk name for touched files")
            .interact_text()?;

        vec![ChunkPlan { name: chunk_name, files: selected }]
    };

    if plan.is_empty() {
        bail!("No chunks defined to touch.");
    }

    // Delegate to split's touch-style applicator
    crate::split::apply_touch_plan(&root, plan)?;

    let final_state = MergesState::load(&root)?;
    println!(
        "{} {} chunk(s) created. Run {} to push.",
        "✓".green().bold(),
        final_state.chunks.len().to_string().yellow(),
        "merges push".bold()
    );

    Ok(())
}
