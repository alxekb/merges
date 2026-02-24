# merges

> Break down huge PRs into small, reviewable chunks — with GitHub PR automation and first-class MCP/LLM support.

[![CI](https://github.com/your-org/merges/actions/workflows/ci.yml/badge.svg)](https://github.com/your-org/merges/actions/workflows/ci.yml)

## The problem

| Pain | How `merges` solves it |
|---|---|
| Huge PRs take hours to review | Split into focused, independent chunks |
| Hard to keep up with fast-moving `main` | Auto-rebase every chunk branch |
| Daily merge goal feels impossible | One command creates all PRs |
| Forgot a file, wrong chunk | `merges add` / `merges move` fix it instantly |
| LLMs can't drive branch management | MCP server — LLMs call `merges` as a tool |

## Install

```bash
# From source (requires Rust ≥ 1.75)
git clone https://github.com/your-org/merges
cd merges
cargo install --path .
```

## Authentication

`merges` resolves a GitHub token automatically:

1. **`gh auth token`** — if the [GitHub CLI](https://cli.github.com/) is logged in  
2. **`GITHUB_TOKEN`** env var — personal access token with `repo` scope

```bash
gh auth login          # recommended
# or
export GITHUB_TOKEN=ghp_...
```

---

## Quickstart

```bash
# 1. On your feature branch, initialise tracking
merges init                     # prompts for base branch (default: main)
merges init --base develop      # or specify explicitly

# 2. Split changed files into chunks (three modes)
merges split                    # interactive TUI — pick files per chunk
merges split --auto             # auto-group by directory structure
merges split --plan '[{"name":"models","files":["src/models/user.rs"]}]'

# 3. Push chunks and create GitHub PRs
merges push --stacked           # each PR targets the previous chunk's branch
merges push --independent       # all PRs target the base branch directly

# 4. Keep chunks in sync when main moves
merges sync

# 5. Check PR status (CI + reviews)
merges status

# 6. Made a mistake? Fix it
merges add models src/models/tag.rs    # add a forgotten file to a chunk

# 7. Clean up after merging
merges clean --merged --yes            # delete branches for merged PRs
```

---

## Commands

### `merges init [--base <branch>]`

Detects the current branch, parses `owner/repo` from the `origin` remote, and writes `.merges.json`. Also adds `.merges.json` to `.git/info/exclude` so it never appears in diffs.

---

### `merges split [--auto | --plan <JSON>]`

Three modes:

| Flag | Behaviour |
|---|---|
| *(none)* | Interactive TUI — name chunks, select files with Space/Enter |
| `--auto` | Auto-groups files by top-level directory (great starting point) |
| `--plan <JSON>` | Non-interactive — apply a pre-built JSON plan (used by MCP/LLM) |

Each mode creates local git branches (e.g. `feat/big-chunk-1-models`) and is **atomic** — if any step fails, all partial branches are rolled back and the state file is unchanged.

**`--auto` grouping rules:**
- All files under a single top dir (e.g. all `src/*`) → grouped by the next level (`src/models/` → `models`, `src/api/` → `api`)
- Files spread across multiple top dirs → grouped by top dir (`frontend/`, `backend/`)
- Root-level files (`Cargo.toml`, `README.md`) → `root` chunk

---

### `merges push [--stacked | --independent]`

For each chunk (in order):
1. Checks out the chunk branch
2. `git fetch origin && git rebase origin/<base>` (keeps it current)
3. `git push --force-with-lease`
4. Creates **or** updates the GitHub PR

| Strategy | PR base |
|---|---|
| `--stacked` | Each PR targets the *previous* chunk's branch — sequential merges |
| `--independent` | All PRs target `<base>` directly — any order |

The strategy is saved in `.merges.json` and reused on subsequent `push` runs.

---

### `merges sync`

Rebases every chunk branch onto `origin/<base>`. Run this whenever colleagues push to `main`.

```
→ Syncing 3 chunk branch(es) onto 'main'
[████████████████████████████████████████] 3/3 done
✓ All chunks are up to date with 'main'.
```

---

### `merges status`

Fetches PR metadata from GitHub and renders a live table:

```
╔═══╦══════════╦═══════════════════════════════╦════╦═════════╦══════════════════╦═══════╗
║ # ║ Chunk    ║ Branch                        ║ PR ║ CI      ║ Review           ║ Files ║
╠═══╬══════════╬═══════════════════════════════╬════╬═════════╬══════════════════╬═══════╣
║ 1 ║ models   ║ feat/big-chunk-1-models       ║ #42║ success ║ approved         ║   3   ║
║ 2 ║ api      ║ feat/big-chunk-2-api          ║ #43║ pending ║ pending          ║   8   ║
║ 3 ║ frontend ║ feat/big-chunk-3-frontend     ║ #44║ failure ║ changes_requested║  12   ║
╚═══╩══════════╩═══════════════════════════════╩════╩═════════╩══════════════════╩═══════╝
```

---

### `merges add <chunk> <file>...`

Add one or more files to an existing chunk. Cherry-picks them from the source branch and amends the chunk commit. Idempotent — adding a file that's already in the chunk is a no-op.

```bash
merges add models src/models/tag.rs src/models/comment.rs
```

---

### `merges clean [--merged] [-y]`

Delete local chunk branches.

| Flag | Behaviour |
|---|---|
| *(none)* | Offers to delete **all** chunk branches |
| `--merged` | Only deletes branches whose GitHub PRs are merged/closed |
| `-y` | Skip confirmation prompt |

Removes cleaned chunks from `.merges.json`.

---

### `merges mcp`

Starts a **stdio MCP server** (JSON-RPC 2.0). Any MCP-compatible LLM client can call `merges` as a set of tools.

---

## MCP / LLM Integration

The recommended LLM workflow with `merges mcp`:

```
1. LLM calls merges_init    → sets up .merges.json
2. LLM calls merges_split   → (no plan) gets back list of changed files
3. LLM decides grouping, calls merges_split again with a plan
4. LLM calls merges_push    → branches pushed, PRs created
5. LLM calls merges_status  → returns JSON status for display
6. LLM calls merges_sync    → keeps chunks rebased
```

### Available MCP tools

| Tool | Input | Output |
|---|---|---|
| `merges_init` | `base_branch?` | Success message |
| `merges_split` | `plan?` (array of `{name, files}`) | Changed files list **or** applied chunks |
| `merges_push` | `strategy?` (`"stacked"` \| `"independent"`) | Push results |
| `merges_sync` | — | Sync results |
| `merges_status` | — | JSON chunk/PR status |

### GitHub Copilot (VS Code)

Add to `.vscode/mcp.json`:

```json
{
  "servers": {
    "merges": {
      "type": "stdio",
      "command": "merges",
      "args": ["mcp"]
    }
  }
}
```

### Claude Desktop

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "merges": {
      "command": "merges",
      "args": ["mcp"]
    }
  }
}
```

---

## Shell completions

```bash
# Bash
merges completions bash >> ~/.bash_completion

# Zsh
merges completions zsh > ~/.zfunc/_merges

# Fish
merges completions fish > ~/.config/fish/completions/merges.fish
```

---

## State file — `.merges.json`

Written to the repo root by `merges init`. Automatically excluded from git via `.git/info/exclude` (never committed, no diffs). Commit it if you want to share chunk definitions with your team.

```json
{
  "base_branch": "main",
  "source_branch": "feat/big-feature",
  "repo_owner": "acme",
  "repo_name": "myrepo",
  "strategy": "stacked",
  "chunks": [
    {
      "name": "models",
      "branch": "feat/big-feature-chunk-1-models",
      "files": ["src/models/user.rs", "src/models/post.rs"],
      "pr_number": 42,
      "pr_url": "https://github.com/acme/myrepo/pull/42"
    },
    {
      "name": "api",
      "branch": "feat/big-feature-chunk-2-api",
      "files": ["src/api/routes.rs", "src/api/handlers.rs"],
      "pr_number": 43,
      "pr_url": "https://github.com/acme/myrepo/pull/43"
    }
  ]
}
```

---

## License

MIT
