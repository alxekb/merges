use anyhow::{bail, Result};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, MultiSelect};
use indicatif::{ProgressBar, ProgressStyle};

use crate::{git, split::{auto_group_files, ChunkPlan}, state::MergesState};

/// Entry point for `merges split`.
///
/// - `plan_json`: if `Some`, parse chunk assignments from JSON and apply non-interactively.
///   Format: `[{"name":"models","files":["src/models/user.rs"]}]`
/// - `auto`: if `true`, automatically group files by directory structure.
/// - Otherwise, fall through to the interactive TUI.
pub fn run(plan_json: Option<String>, auto: bool) -> Result<()> {
    let root = git::repo_root()?;
    let state = MergesState::load(&root)?;

    let all_files = git::changed_files(&root, &state.base_branch)?;
    if all_files.is_empty() {
        bail!(
            "No changed files found between HEAD and '{}'",
            state.base_branch
        );
    }

    println!(
        "{} Found {} changed file(s) on '{}' vs '{}'",
        "→".blue().bold(),
        all_files.len().to_string().yellow(),
        state.source_branch.cyan(),
        state.base_branch.cyan()
    );

    if auto {
        // ── Auto-group path ───────────────────────────────────────────────
        let plan = auto_group_files(&all_files);
        println!(
            "{} Auto-grouped into {} chunk(s):",
            "→".blue().bold(),
            plan.len().to_string().yellow()
        );
        for (i, chunk) in plan.iter().enumerate() {
            println!(
                "  {}. {} ({} files)",
                i + 1,
                chunk.name.cyan(),
                chunk.files.len().to_string().yellow()
            );
        }

        let pb = ProgressBar::new(plan.len() as u64);
        pb.set_style(ProgressStyle::default_bar()
            .template("{bar:40.cyan/blue} {pos}/{len} chunks {msg}")
            .unwrap());

        crate::split::apply_plan(&root, plan)?;
        pb.finish_with_message("done");

        let state = MergesState::load(&root)?;
        println!(
            "{} {} chunk(s) created. Run {} to push.",
            "✓".green().bold(),
            state.chunks.len().to_string().yellow(),
            "merges push".bold()
        );
        return Ok(());
    }

    if let Some(json) = plan_json {
        // ── Non-interactive path (MCP / LLM / --plan flag) ────────────────
        let plan: Vec<ChunkPlan> = serde_json::from_str(&json)
            .map_err(|e| anyhow::anyhow!("Invalid --plan JSON: {}", e))?;

        let pb = ProgressBar::new(plan.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{bar:40.cyan/blue} {pos}/{len} chunks {msg}")
                .unwrap(),
        );

        crate::split::apply_plan(&root, plan)?;
        pb.finish_with_message("done");

        let state = MergesState::load(&root)?;
        println!(
            "{} {} chunk(s) created. Run {} to push.",
            "✓".green().bold(),
            state.chunks.len().to_string().yellow(),
            "merges push".bold()
        );
    } else {
        // ── Interactive TUI path ──────────────────────────────────────────
        run_interactive(&root, &state, &all_files)?;
    }

    Ok(())
}

fn run_interactive(
    root: &std::path::Path,
    state: &MergesState,
    all_files: &[String],
) -> Result<()> {
    let mut assigned: Vec<String> = state
        .chunks
        .iter()
        .flat_map(|c| c.files.iter().cloned())
        .collect();

    let mut new_plans: Vec<ChunkPlan> = vec![];

    loop {
        let remaining: Vec<String> = all_files
            .iter()
            .filter(|f| !assigned.contains(f))
            .cloned()
            .collect();

        if remaining.is_empty() {
            println!("{} All files have been assigned to chunks.", "✓".green().bold());
            break;
        }

        println!(
            "\n{} remaining file(s) unassigned. Define a new chunk (or Ctrl-C to stop):",
            remaining.len().to_string().yellow()
        );

        let chunk_name: String = Input::new()
            .with_prompt("Chunk name (e.g. models, api, frontend)")
            .interact_text()?;

        let selections = MultiSelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Select files (Space = toggle, Enter = confirm)")
            .items(&remaining)
            .interact()?;

        if selections.is_empty() {
            let stop = Confirm::new()
                .with_prompt("No files selected — stop assigning chunks?")
                .default(false)
                .interact()?;
            if stop {
                break;
            }
            continue;
        }

        let selected_files: Vec<String> = selections.iter().map(|&i| remaining[i].clone()).collect();
        assigned.extend(selected_files.clone());
        new_plans.push(ChunkPlan { name: chunk_name, files: selected_files });

        let more = Confirm::new()
            .with_prompt("Add another chunk?")
            .default(true)
            .interact()?;
        if !more {
            break;
        }
    }

    if new_plans.is_empty() {
        println!("{} No new chunks defined.", "!".yellow().bold());
        return Ok(());
    }

    // Apply all the interactively-defined chunks
    crate::split::apply_plan(root, new_plans)?;

    let unassigned: Vec<_> = all_files.iter().filter(|f| !assigned.contains(f)).collect();
    if !unassigned.is_empty() {
        println!(
            "\n{} {} file(s) not assigned to any chunk:",
            "!".yellow().bold(),
            unassigned.len()
        );
        for f in &unassigned {
            println!("  {}", f.dimmed());
        }
    }

    let final_state = MergesState::load(root)?;
    println!(
        "\n{} {} chunk(s) defined. Run {} to push.",
        "✓".green().bold(),
        final_state.chunks.len().to_string().yellow(),
        "merges push".bold()
    );

    Ok(())
}
