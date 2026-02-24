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
                Detects the source branch and sets up .merges.json."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "base_branch": {
                        "type": "string",
                        "description": "The base branch PRs will target (default: main)"
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
    ]
}
