use anyhow::{bail, Result};
use dialoguer::{theme::ColorfulTheme, MultiSelect};

/// Prompt the user to select one or more files from a list.
pub fn select_files(prompt: &str, files: &[String]) -> Result<Vec<String>> {
    if files.is_empty() {
        bail!("No files available to select.");
    }

    let selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("{} (Space = toggle, Enter = confirm)", prompt))
        .items(files)
        .interact()?;

    if selections.is_empty() {
        return Ok(vec![]);
    }

    Ok(selections.iter().map(|&i| files[i].clone()).collect())
}
