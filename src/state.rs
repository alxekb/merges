use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const STATE_FILE: &str = ".merges.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    Stacked,
    Independent,
}

impl std::fmt::Display for Strategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Strategy::Stacked => write!(f, "stacked"),
            Strategy::Independent => write!(f, "independent"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub name: String,
    pub branch: String,
    pub files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergesState {
    pub base_branch: String,
    pub source_branch: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub strategy: Strategy,
    pub chunks: Vec<Chunk>,
}

impl MergesState {
    pub fn load(repo_root: &Path) -> Result<Self> {
        let path = repo_root.join(STATE_FILE);
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Could not read {}. Run `merges init` first.", STATE_FILE))?;
        serde_json::from_str(&content).context("Failed to parse .merges.json")
    }

    pub fn save(&self, repo_root: &Path) -> Result<()> {
        let path = repo_root.join(STATE_FILE);
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write {}", path.display()))
    }

    pub fn path(repo_root: &Path) -> PathBuf {
        repo_root.join(STATE_FILE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_state() -> MergesState {
        MergesState {
            base_branch: "main".to_string(),
            source_branch: "feat/big-feature".to_string(),
            repo_owner: "acme".to_string(),
            repo_name: "myrepo".to_string(),
            strategy: Strategy::Stacked,
            chunks: vec![],
        }
    }

    fn sample_chunk_without_pr() -> Chunk {
        Chunk {
            name: "models".to_string(),
            branch: "feat/big-feature-chunk-1-models".to_string(),
            files: vec!["src/models/user.rs".to_string()],
            pr_number: None,
            pr_url: None,
        }
    }

    fn sample_chunk_with_pr() -> Chunk {
        Chunk {
            name: "api".to_string(),
            branch: "feat/big-feature-chunk-2-api".to_string(),
            files: vec!["src/api/routes.rs".to_string(), "src/api/handlers.rs".to_string()],
            pr_number: Some(42),
            pr_url: Some("https://github.com/acme/myrepo/pull/42".to_string()),
        }
    }

    // ── Strategy ─────────────────────────────────────────────────────────

    #[test]
    fn test_strategy_display_stacked() {
        assert_eq!(Strategy::Stacked.to_string(), "stacked");
    }

    #[test]
    fn test_strategy_display_independent() {
        assert_eq!(Strategy::Independent.to_string(), "independent");
    }

    #[test]
    fn test_strategy_equality() {
        assert_eq!(Strategy::Stacked, Strategy::Stacked);
        assert_ne!(Strategy::Stacked, Strategy::Independent);
    }

    #[test]
    fn test_strategy_serializes_as_snake_case() {
        let json = serde_json::to_string(&Strategy::Stacked).unwrap();
        assert_eq!(json, r#""stacked""#);

        let json = serde_json::to_string(&Strategy::Independent).unwrap();
        assert_eq!(json, r#""independent""#);
    }

    #[test]
    fn test_strategy_deserializes_from_snake_case() {
        let s: Strategy = serde_json::from_str(r#""stacked""#).unwrap();
        assert_eq!(s, Strategy::Stacked);

        let s: Strategy = serde_json::from_str(r#""independent""#).unwrap();
        assert_eq!(s, Strategy::Independent);
    }

    // ── Chunk ─────────────────────────────────────────────────────────────

    #[test]
    fn test_chunk_without_pr_omits_optional_fields_in_json() {
        let chunk = sample_chunk_without_pr();
        let json = serde_json::to_string(&chunk).unwrap();
        assert!(!json.contains("pr_number"), "pr_number should be omitted when None");
        assert!(!json.contains("pr_url"), "pr_url should be omitted when None");
    }

    #[test]
    fn test_chunk_with_pr_includes_optional_fields_in_json() {
        let chunk = sample_chunk_with_pr();
        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("pr_number"));
        assert!(json.contains("pr_url"));
        assert!(json.contains("42"));
    }

    #[test]
    fn test_chunk_roundtrip_serialization_without_pr() {
        let original = sample_chunk_without_pr();
        let json = serde_json::to_string(&original).unwrap();
        let restored: Chunk = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, original.name);
        assert_eq!(restored.branch, original.branch);
        assert_eq!(restored.files, original.files);
        assert_eq!(restored.pr_number, None);
        assert_eq!(restored.pr_url, None);
    }

    #[test]
    fn test_chunk_roundtrip_serialization_with_pr() {
        let original = sample_chunk_with_pr();
        let json = serde_json::to_string(&original).unwrap();
        let restored: Chunk = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.pr_number, Some(42));
        assert_eq!(restored.pr_url, Some("https://github.com/acme/myrepo/pull/42".to_string()));
    }

    // ── MergesState ───────────────────────────────────────────────────────

    #[test]
    fn test_state_serializes_all_fields() {
        let state = sample_state();
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("base_branch"));
        assert!(json.contains("source_branch"));
        assert!(json.contains("repo_owner"));
        assert!(json.contains("repo_name"));
        assert!(json.contains("strategy"));
        assert!(json.contains("chunks"));
    }

    #[test]
    fn test_state_roundtrip_serialization() {
        let original = sample_state();
        let json = serde_json::to_string(&original).unwrap();
        let restored: MergesState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.base_branch, "main");
        assert_eq!(restored.source_branch, "feat/big-feature");
        assert_eq!(restored.repo_owner, "acme");
        assert_eq!(restored.repo_name, "myrepo");
        assert_eq!(restored.strategy, Strategy::Stacked);
        assert!(restored.chunks.is_empty());
    }

    #[test]
    fn test_state_with_chunks_roundtrip() {
        let mut state = sample_state();
        state.chunks.push(sample_chunk_without_pr());
        state.chunks.push(sample_chunk_with_pr());

        let json = serde_json::to_string(&state).unwrap();
        let restored: MergesState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.chunks.len(), 2);
        assert_eq!(restored.chunks[0].name, "models");
        assert_eq!(restored.chunks[1].pr_number, Some(42));
    }

    // ── save / load ───────────────────────────────────────────────────────

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let mut state = sample_state();
        state.chunks.push(sample_chunk_with_pr());

        state.save(dir.path()).unwrap();

        assert!(dir.path().join(STATE_FILE).exists(), ".merges.json should exist after save");

        let loaded = MergesState::load(dir.path()).unwrap();
        assert_eq!(loaded.base_branch, state.base_branch);
        assert_eq!(loaded.source_branch, state.source_branch);
        assert_eq!(loaded.chunks.len(), 1);
        assert_eq!(loaded.chunks[0].pr_number, Some(42));
    }

    #[test]
    fn test_load_missing_file_returns_error_with_hint() {
        let dir = TempDir::new().unwrap();
        let result = MergesState::load(dir.path());
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("merges init"),
            "Error should hint to run `merges init`, got: {}",
            msg
        );
    }

    #[test]
    fn test_load_invalid_json_returns_error() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(STATE_FILE), "not valid json {{").unwrap();
        let result = MergesState::load(dir.path());
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("parse") || msg.contains("JSON"), "Got: {}", msg);
    }

    #[test]
    fn test_path_returns_state_file_in_repo_root() {
        let dir = TempDir::new().unwrap();
        let path = MergesState::path(dir.path());
        assert_eq!(path, dir.path().join(".merges.json"));
    }

    #[test]
    fn test_save_overwrites_existing_state_file() {
        let dir = TempDir::new().unwrap();
        let mut state = sample_state();
        state.save(dir.path()).unwrap();

        state.base_branch = "develop".to_string();
        state.save(dir.path()).unwrap();

        let loaded = MergesState::load(dir.path()).unwrap();
        assert_eq!(loaded.base_branch, "develop");
    }

    /// ❌ RED: saved JSON should be pretty-printed (human-readable), not minified.
    #[test]
    fn test_save_produces_pretty_json() {
        let dir = TempDir::new().unwrap();
        let state = sample_state();
        state.save(dir.path()).unwrap();

        let raw = std::fs::read_to_string(dir.path().join(STATE_FILE)).unwrap();
        assert!(raw.contains('\n'), "Saved JSON should be pretty-printed with newlines");
        assert!(raw.contains("  "), "Saved JSON should be indented");
    }
}
