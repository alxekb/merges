use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// MCP tool descriptor following the Model Context Protocol spec.
#[derive(Debug, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

pub fn all_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "merges_init".to_string(),
            description: "Initialise merges tracking for the current git repository. \
                Detects the source branch and sets up .merges.json. \
                Pass commit_prefix to override auto-detected ticket prefix for commit messages and PR titles."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "base_branch": {
                        "type": "string",
                        "description": "The base branch PRs will target (default: main)"
                    },
                    "commit_prefix": {
                        "type": "string",
                        "description": "Explicit prefix for all commit messages and PR titles (e.g. JCLARK-97246). Auto-detected from branch name if omitted."
                    }
                }
            }),
        },
        Tool {
            name: "merges_split".to_string(),
            description: "Split changed files into named chunks and create local git branches. \
                Call without 'plan' first to get the list of changed files, then call again \
                with a 'plan' to apply your chunk assignments."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "plan": {
                        "type": "array",
                        "description": "Chunk assignments to apply. If omitted, returns changed files for inspection.",
                        "items": {
                            "type": "object",
                            "required": ["name", "files"],
                            "properties": {
                                "name": {
                                    "type": "string",
                                    "description": "Short name for this chunk (e.g. 'models', 'api')"
                                },
                                "files": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "description": "Relative file paths to include in this chunk"
                                }
                            }
                        }
                    }
                }
            }),
        },
        Tool {
            name: "merges_push".to_string(),
            description: "Push chunk branches to origin and create or update GitHub PRs. \
                Auto-rebases each branch onto the latest base branch."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "strategy": {
                        "type": "string",
                        "enum": ["stacked", "independent"],
                        "description": "PR topology: stacked (each PR targets the previous chunk) or independent (all target base)"
                    }
                }
            }),
        },
        Tool {
            name: "merges_sync".to_string(),
            description: "Rebase all chunk branches onto the latest base branch (e.g. main). \
                Keeps chunks in sync with a fast-moving main."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        Tool {
            name: "merges_status".to_string(),
            description: "Return a JSON summary of all chunks: branch, PR number, PR URL, \
                CI status, and review state."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        Tool {
            name: "merges_add".to_string(),
            description: "Add files to an existing chunk (amends its branch commit). \
                Use after split to move forgotten files into a chunk without re-splitting."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "required": ["chunk", "files"],
                "properties": {
                    "chunk": {
                        "type": "string",
                        "description": "Name of the existing chunk to add files to"
                    },
                    "files": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Relative file paths to add to this chunk"
                    }
                }
            }),
        },
        Tool {
            name: "merges_move".to_string(),
            description: "Move a file from one chunk to another atomically. \
                Removes the file from the source chunk branch and adds it to the destination."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "required": ["file", "from", "to"],
                "properties": {
                    "file": {
                        "type": "string",
                        "description": "Relative path of the file to move"
                    },
                    "from": {
                        "type": "string",
                        "description": "Name of the source chunk"
                    },
                    "to": {
                        "type": "string",
                        "description": "Name of the destination chunk"
                    }
                }
            }),
        },
        Tool {
            name: "merges_clean".to_string(),
            description: "Delete local chunk branches. Pass dry_run:true to preview. \
                Pass merged:true to only delete branches whose GitHub PRs are merged."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "merged": {
                        "type": "boolean",
                        "description": "Only delete branches for merged/closed PRs"
                    },
                    "dry_run": {
                        "type": "boolean",
                        "description": "Return list of branches that would be deleted, without deleting"
                    }
                }
            }),
        },
        Tool {
            name: "merges_doctor".to_string(),
            description: "Validate state consistency: branch existence, worktrees, gitignore, \
                duplicate file assignments. Returns a JSON report. Pass repair:true to auto-fix."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repair": {
                        "type": "boolean",
                        "description": "Attempt to repair detected issues (e.g. re-add gitignore entry)"
                    }
                }
            }),
        },
    ]
}
