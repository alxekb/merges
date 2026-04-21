use anyhow::Result;
use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, Color, ContentArrangement, Table};

use crate::{config, git, github, state::MergesState};

pub async fn run() -> Result<()> {
    let root = git::repo_root()?;
    let state = MergesState::load(&root)?;

    if state.chunks.is_empty() {
        println!("No chunks defined yet. Run {} first.", "merges split".bold());
        return Ok(());
    }

    println!(
        "{} Status for {}/{} — source: {}, base: {}",
        "→".blue().bold(),
        state.repo_owner.cyan(),
        state.repo_name.cyan(),
        state.source_branch.yellow(),
        state.base_branch.yellow()
    );

    let token = config::github_token().ok();
    let gh = token.as_deref().and_then(|t| github::client(t).ok());

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("#").add_attribute(Attribute::Bold),
            Cell::new("Chunk").add_attribute(Attribute::Bold),
            Cell::new("Branch").add_attribute(Attribute::Bold),
            Cell::new("Sync").add_attribute(Attribute::Bold),
            Cell::new("PR").add_attribute(Attribute::Bold),
            Cell::new("CI").add_attribute(Attribute::Bold),
            Cell::new("Review").add_attribute(Attribute::Bold),
            Cell::new("Files").add_attribute(Attribute::Bold),
        ]);

    for (i, chunk) in state.chunks.iter().enumerate() {
        let (pr_cell, ci_cell, review_cell) = if let (Some(gh_client), Some(pr_num)) = (&gh, chunk.pr_number) {
            match github::get_pr_info(gh_client, &state.repo_owner, &state.repo_name, pr_num).await {
                Ok(info) => {
                    let mut pr_text = format!("#{}", info.number);
                    if info.is_merged {
                        pr_text.push_str(" (merged)");
                    } else if info.state == "closed" {
                        pr_text.push_str(" (closed)");
                    }
                    (pr_text, info.ci_status, info.review_state)
                }
                Err(_) => ("error".to_string(), "error".to_string(), "error".to_string()),
            }
        } else {
            let pr_text = chunk.pr_number.map(|n| format!("#{}", n)).unwrap_or_else(|| "—".to_string());
            (pr_text, "—".to_string(), "—".to_string())
        };

        // Check local branch existence
        let branch_exists = git::branch_exists(&root, &chunk.branch).unwrap_or(false);
        let branch_cell = if branch_exists {
            chunk.branch.cyan()
        } else {
            format!("{} (deleted)", chunk.branch).red()
        };

        let behind = if branch_exists {
            git::commits_behind(&root, &chunk.branch, &state.base_branch).unwrap_or(0)
        } else {
            0
        };
        let sync_label = if branch_exists {
            git::sync_status(behind)
        } else {
            "—".to_string()
        };
        let sync_color = if !branch_exists {
            Color::Reset
        } else if behind == 0 {
            Color::Green
        } else {
            Color::Yellow
        };

        let pr_color = if pr_cell.contains("(merged)") {
            Color::Green
        } else if pr_cell.contains("(closed)") {
            Color::Red
        } else {
            Color::Reset
        };

        let ci_color = match ci_cell.as_str() {
            "success" => Color::Green,
            "failure" | "error" => Color::Red,
            _ => Color::Yellow,
        };

        let review_color = match review_cell.as_str() {
            "approved" => Color::Green,
            "changes_requested" => Color::Red,
            "pending" => Color::Yellow,
            _ => Color::Reset,
        };

        table.add_row(vec![
            Cell::new(i + 1),
            Cell::new(&chunk.name),
            Cell::new(branch_cell),
            Cell::new(&sync_label).fg(sync_color),
            Cell::new(&pr_cell).fg(pr_color),
            Cell::new(&ci_cell).fg(ci_color),
            Cell::new(&review_cell).fg(review_color),
            Cell::new(chunk.files.len()),
        ]);
    }

    println!("{}", table);

    if let Some(url) = state.chunks.first().and_then(|c| c.pr_url.as_deref()) {
        println!(
            "\n  First PR: {}",
            url.dimmed()
        );
    }

    Ok(())
}
