//! TDD tests for the auto-split file grouping logic.
//! These tests fail until auto_group_files is implemented.

use merges::split::{auto_group_files, ChunkPlan};

fn sorted(mut plans: Vec<ChunkPlan>) -> Vec<ChunkPlan> {
    plans.sort_by(|a, b| a.name.cmp(&b.name));
    for p in &mut plans {
        p.files.sort();
    }
    plans
}

// ── auto_group_files ────────────────────────────────────────────────────────

#[test]
fn test_auto_group_by_top_directory() {
    let files = vec![
        "src/models/user.rs".to_string(),
        "src/models/post.rs".to_string(),
        "src/api/routes.rs".to_string(),
        "src/api/handlers.rs".to_string(),
    ];
    let plans = auto_group_files(&files);
    let plans = sorted(plans);

    assert_eq!(plans.len(), 2, "Should produce 2 chunks (models, api)");
    assert_eq!(plans[0].name, "api");
    assert_eq!(plans[0].files, vec!["src/api/handlers.rs", "src/api/routes.rs"]);
    assert_eq!(plans[1].name, "models");
    assert_eq!(plans[1].files, vec!["src/models/post.rs", "src/models/user.rs"]);
}

#[test]
fn test_auto_group_root_files_go_into_root_chunk() {
    let files = vec![
        "Cargo.toml".to_string(),
        "README.md".to_string(),
        "src/lib.rs".to_string(),
    ];
    let plans = auto_group_files(&files);
    let plan_names: Vec<&str> = plans.iter().map(|p| p.name.as_str()).collect();

    // Root-level files (no directory) should form a "root" chunk
    assert!(plan_names.contains(&"root"), "Root files should go into 'root' chunk, got: {:?}", plan_names);

    let root_chunk = plans.iter().find(|p| p.name == "root").unwrap();
    assert!(root_chunk.files.contains(&"Cargo.toml".to_string()));
    assert!(root_chunk.files.contains(&"README.md".to_string()));
}

#[test]
fn test_auto_group_single_directory_single_chunk() {
    let files = vec!["src/foo.rs".to_string(), "src/bar.rs".to_string()];
    let plans = auto_group_files(&files);

    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].name, "src");
}

#[test]
fn test_auto_group_empty_list_returns_empty() {
    let plans = auto_group_files(&[]);
    assert!(plans.is_empty());
}

#[test]
fn test_auto_group_mixed_depth_uses_top_level_dir() {
    // Files at different depths should still group by top-level dir
    let files = vec![
        "frontend/components/Button.tsx".to_string(),
        "frontend/pages/index.tsx".to_string(),
        "frontend/utils/helpers.ts".to_string(),
        "backend/server.rs".to_string(),
    ];
    let plans = sorted(auto_group_files(&files));

    assert_eq!(plans.len(), 2);
    assert_eq!(plans[0].name, "backend");
    assert_eq!(plans[1].name, "frontend");
    assert_eq!(plans[1].files.len(), 3);
}

#[test]
fn test_auto_group_preserves_all_files() {
    let files = vec![
        "a/x.rs".to_string(), "a/y.rs".to_string(),
        "b/z.rs".to_string(), "README.md".to_string(),
    ];
    let plans = auto_group_files(&files);
    let total_files: usize = plans.iter().map(|p| p.files.len()).sum();
    assert_eq!(total_files, 4, "All files should appear in exactly one chunk");
}

#[test]
fn test_auto_group_each_file_in_exactly_one_chunk() {
    let files = vec![
        "x/a.rs".to_string(), "x/b.rs".to_string(), "y/c.rs".to_string(),
    ];
    let plans = auto_group_files(&files);

    // Collect all files across chunks and check no duplicates
    let mut all: Vec<String> = plans.iter().flat_map(|p| p.files.iter().cloned()).collect();
    all.sort();
    let deduped: Vec<_> = all.iter().cloned().collect::<std::collections::HashSet<_>>().into_iter().collect();
    assert_eq!(all.len(), deduped.len(), "Each file should appear in exactly one chunk");
}
