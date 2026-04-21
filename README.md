# merges

> Break a large feature branch into small, reviewable PRs — with automatic branch management, GitHub PR creation, first-class MCP/LLM support, and full coding-agent integration.

[![CI](https://github.com/alxekb/merges/actions/workflows/ci.yml/badge.svg)](https://github.com/alxekb/merges/actions/workflows/ci.yml)

## The problem

You've been working on `feat/payments-v2` for two weeks. It touches 40 files across the database layer, API routes, frontend components, and tests. Your EM wants daily merges. Your reviewer wants something they can actually read.

`merges` splits that branch into a chain of small PRs — each one reviewable in 15 minutes — while keeping everything rebased on `main` automatically.

---

## How it works (concrete example)

Say your branch `feat/payments-v2` has these changed files:

```
db/migrations/001_add_payments.sql
db/migrations/002_add_refunds.sql
src/models/payment.rs
src/models/refund.rs
src/api/payments.rs
src/api/refunds.rs
src/api/webhooks.rs
frontend/components/PaymentForm.tsx
frontend/components/RefundModal.tsx
frontend/pages/checkout.tsx
tests/integration/payments_test.rs
tests/integration/refunds_test.rs
```

You run:

```bash
merges init
merges split --auto
merges push --stacked
```

**What happens:**

### `merges init`

Reads your git remote (`git@github.com:acme/myapp.git`) and creates `.merges.json`:

```json
{
  "base_branch": "main",
  "source_branch": "feat/payments-v2",
  "repo_owner": "acme",
  "repo_name": "myapp",
  "strategy": "stacked",
  "chunks": []
}
```

Also runs `git config rerere.enabled true` — so any merge conflict you resolve once is never shown to you again on the same file.

### `merges split --auto`

Groups files by directory (second level when all files share one top dir):

| Chunk | Branch | Files |
|---|---|---|
| `db` | `feat/payments-v2-chunk-1-db` | `db/migrations/001_add_payments.sql`, `db/migrations/002_add_refunds.sql` |
| `models` | `feat/payments-v2-chunk-2-models` | `src/models/payment.rs`, `src/models/refund.rs` |
| `api` | `feat/payments-v2-chunk-3-api` | `src/api/payments.rs`, `src/api/refunds.rs`, `src/api/webhooks.rs` |
| `frontend` | `feat/payments-v2-chunk-4-frontend` | `frontend/components/PaymentForm.tsx`, `frontend/components/RefundModal.tsx`, `frontend/pages/checkout.tsx` |
| `tests` | `feat/payments-v2-chunk-5-tests` | `tests/integration/payments_test.rs`, `tests/integration/refunds_test.rs` |

For each chunk, `merges split` checks out the new branch from the merge-base with `main`, cherry-picks only those files, and creates a commit:

```
feat/payments-v2 ──────────────────────────────── (your 40-file branch)
      │
      ├── feat/payments-v2-chunk-1-db             (2 files from db/)
      ├── feat/payments-v2-chunk-2-models          (2 files from src/models/)
      ├── feat/payments-v2-chunk-3-api             (3 files from src/api/)
      ├── feat/payments-v2-chunk-4-frontend        (3 files from frontend/)
      └── feat/payments-v2-chunk-5-tests           (2 files from tests/)
```

### `merges push --stacked`

For each chunk (in order), `merges push`:
1. Rebases the chunk branch onto `origin/main` with `--update-refs` (stacked branches update automatically)
2. Force-pushes with `--force-with-lease`
3. Creates a GitHub PR

**Stacked** means each PR targets the previous chunk's branch:

```
main ← PR#1: feat/payments-v2-chunk-1-db
             ↑ PR#2: feat/payments-v2-chunk-2-models
                      ↑ PR#3: feat/payments-v2-chunk-3-api
                               ↑ PR#4: feat/payments-v2-chunk-4-frontend
                                        ↑ PR#5: feat/payments-v2-chunk-5-tests
```

Each PR is small and reviewable. When PR#1 merges into `main`, PR#2's base automatically re-targets `main` (via `merges push` or `merges sync`).

**Independent** mode (`merges push --independent`) points every PR directly at `main`:

```
main ← PR#1: feat/payments-v2-chunk-1-db
main ← PR#2: feat/payments-v2-chunk-2-models
main ← PR#3: feat/payments-v2-chunk-3-api
main ← PR#4: feat/payments-v2-chunk-4-frontend
main ← PR#5: feat/payments-v2-chunk-5-tests
```

Any PR can merge in any order. Good when chunks are truly independent.

---

## Install

### macOS / Linux — one-liner (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/alxekb/merges/main/install.sh | sh
```

Detects your OS and architecture, downloads the right pre-built binary from the [latest GitHub Release](https://github.com/alxekb/merges/releases/latest), and installs it to `/usr/local/bin`.

To install to a different directory:
```bash
MERGES_INSTALL_DIR=~/.local/bin curl -fsSL https://raw.githubusercontent.com/alxekb/merges/main/install.sh | sh
```

### Manual download

Download the binary for your platform from [GitHub Releases](https://github.com/alxekb/merges/releases/latest):

| Platform | File |
|----------|------|
| macOS Apple Silicon | `merges-macos-aarch64` |
| macOS Intel | `merges-macos-x86_64` |
| Linux x86_64 | `merges-linux-x86_64` |
| Linux ARM64 | `merges-linux-aarch64` |

Each binary ships with a `.sha256` checksum file. Verify before running:
```bash
sha256sum -c merges-linux-x86_64.sha256
chmod +x merges-linux-x86_64
sudo mv merges-linux-x86_64 /usr/local/bin/merges
```

### From source (requires Rust ≥ 1.85)

```bash
cargo install --git https://github.com/alxekb/merges merges
```

## Authentication

`merges` resolves a GitHub token in this order:

1. **`gh auth token`** — if the [GitHub CLI](https://cli.github.com/) is installed and logged in
2. **`GITHUB_TOKEN`** env var — personal access token with `repo` scope

```bash
gh auth login          # recommended
# or
export GITHUB_TOKEN=ghp_...
```

---

## Commands

### `merges init [--base <branch>] [--commit-prefix <prefix>]`

Initialises `merges` for the current repo. Detects the current branch and GitHub remote automatically.

| Flag | Description |
|------|-------------|
| `--base <branch>` | Base branch PRs will target (default: `main`) |
| `--commit-prefix <prefix>` | Prefix prepended to every commit message and PR title (e.g. `PROJ-1234`). Auto-detected from the branch name if omitted — e.g. branch `feat/PROJ-1234-payments` yields prefix `PROJ-1234`. |

```
$ git checkout feat/payments-v2
$ merges init --base main

✓ Initialised merges for acme/myapp — source: feat/payments-v2, base: main
  · rerere enabled — conflict resolutions will be replayed automatically.
  Next: run merges split to assign files to chunks.
```

Creates `.merges.json` in the repo root. This file is added to `.git/info/exclude` automatically — it never appears in your diffs or gets accidentally committed.

Also enables `rerere.enabled` and `rerere.autoupdate` locally: resolve a conflict once, and git silently applies the same resolution on every subsequent rebase.

---

### `merges split`

Three modes — choose whichever fits your workflow.

#### Interactive TUI (default)

```
$ merges split

→ Found 12 changed file(s) on 'feat/payments-v2' vs 'main'

Chunk name: db
Select files (space to toggle, enter to confirm):
  ▸ [x] db/migrations/001_add_payments.sql
    [x] db/migrations/002_add_refunds.sql
    [ ] src/models/payment.rs
    [ ] src/api/payments.rs
    ...

✓ Created branch feat/payments-v2-chunk-1-db (2 files)

Chunk name: models
...
```

Each named chunk becomes a branch: `<source-branch>-chunk-<N>-<name>`.

#### `--auto` (directory-based grouping)

```
$ merges split --auto

→ Found 12 changed file(s) on 'feat/payments-v2' vs 'main'
→ Auto-grouped into 5 chunk(s):
  1. db        (2 files)
  2. models    (2 files)
  3. api       (3 files)
  4. frontend  (3 files)
  5. tests     (2 files)
[████████████████████████████████████████] 5/5 chunks done
✓ 5 chunk(s) created. Run merges push to push.
```

**Grouping rules:**
- All changed files live under one top-level dir (e.g. all under `src/`) → group by the *second* level (`src/models/` → `models`, `src/api/` → `api`)
- Files span multiple top-level dirs (`frontend/`, `backend/`, `db/`) → group by top-level dir
- Root-level files (`Cargo.toml`, `package.json`, `README.md`) → `root` chunk

#### `--plan <JSON>` (non-interactive / scripting / MCP)

```bash
merges split --plan '[
  {"name": "db",       "files": ["db/migrations/001_add_payments.sql", "db/migrations/002_add_refunds.sql"]},
  {"name": "models",   "files": ["src/models/payment.rs", "src/models/refund.rs"]},
  {"name": "api",      "files": ["src/api/payments.rs", "src/api/refunds.rs", "src/api/webhooks.rs"]},
  {"name": "frontend", "files": ["frontend/components/PaymentForm.tsx", "frontend/components/RefundModal.tsx", "frontend/pages/checkout.tsx"]},
  {"name": "tests",    "files": ["tests/integration/payments_test.rs", "tests/integration/refunds_test.rs"]}
]'
```

If any branch creation fails mid-way, all partially created branches are rolled back and the state file stays clean.

---

### `merges touch`

Create branches/worktrees and touch (create empty) files from a plan JSON, then commit them on the new branch. Useful to define project scaffolding and open an initial PR quickly.

Usage (non-interactive):

```bash
merges touch --plan '[{"name":"scaffold","files":["src/a.rs","src/b.rs"]}]'
```

Behavior:

- Creates a branch named `<source-branch>-chunk-<N>-<name>` at the merge-base with the base branch.
- In worktree mode, creates a worktree for the branch; otherwise checks out the branch temporarily.
- For each file in the plan, creates parent directories and an empty file if it doesn't exist.
- Commits the touched files using the same commit message formatting as `merges split` (ticket prefix detection / commit_prefix is respected), ensuring compatibility with repo hooks.
- On error, rolls back any created branches or worktrees.

### `merges push [--stacked | --independent]`

```
$ merges push --stacked

→ Pushing 5 chunk(s) as stacked PRs
✓ [db]       PR #101 already merged — skipping
⠸ [models]   Rebasing onto 'main'…
✓ [models]   PR #102 created → https://github.com/acme/myapp/pull/102
...
```

For each chunk:
1. `git checkout feat/payments-v2-chunk-N-<name>`
2. `git fetch origin && git rebase --update-refs origin/main`  ← `--update-refs` keeps the whole stack aligned
3. `git push origin feat/payments-v2-chunk-N-<name> --force-with-lease`
4. Creates (or updates) a GitHub PR

**Robust Automation:**
- **Skip Merged:** If a chunk is already merged, `merges push` skips it and automatically re-stacks subsequent chunks onto the base branch (e.g., `main`).
- **Adopt Existing:** If a PR already exists on GitHub for a branch but isn't in your local state, `merges` will automatically find and adopt it (as long as it was generated by `merges`).
- **Protect Human PRs:** `merges` will never overwrite a PR that doesn't have the `*Generated by [merges]*` signature in its body.

---

### `merges sync`

Run this whenever `main` gets new commits.

```
$ merges sync

→ Syncing 5 chunk branch(es) onto 'main'
[████████████████████████████████████████] 5/5 done
✓ All chunks are up to date with 'main'.
```

In stacked mode, `--update-refs` means rebasing `chunk-1` also slides `chunk-2` through `chunk-5` forward in one pass — you don't need to rebase each branch individually.

If you hit a conflict: resolve it, `git rebase --continue`, then re-run `merges sync`. Because `rerere` is enabled, the same conflict will be auto-resolved on every subsequent sync.

---

### `merges status`

```
$ merges status

╔═══╦══════════╦══════════════════════════════════════╦═══════════╦═══════════════╦═════════╦══════════════════╦═══════╗
║ # ║ Chunk    ║ Branch                               ║ Sync      ║ PR            ║ CI      ║ Review           ║ Files ║
╠═══╬══════════╬══════════════════════════════════════╬═══════════╬═══════════════╬═════════╬══════════════════╬═══════╣
║ 1 ║ db       ║ feat/payments-v2-chunk-1-db (deleted)║ —         ║ #101 (merged) ║ success ║ approved         ║   2   ║
║ 2 ║ models   ║ feat/payments-v2-chunk-2-models      ║ ✓ current ║ #102          ║ success ║ approved         ║   2   ║
║ 3 ║ api      ║ feat/payments-v2-chunk-3-api         ║ ↓ 2 behind║ #103          ║ pending ║ pending          ║   3   ║
...
```

The **Sync** column shows `✓ current` (green) when the chunk branch is up-to-date with the base branch, or `↓ N behind` (yellow) when the base has moved ahead.

It also detects if local branches have been **deleted** or if PRs have been **merged** or **closed** on GitHub.

---

### `merges add [--chunk <name> <file>...]`

You forgot `src/models/payment_method.rs` and it should be in the `models` chunk.

**Interactive UI (default):**
```
$ merges add
? Add files TO chunk: models (2 files)
? Select files to add to 'models' (Space = toggle, Enter = confirm)
  ▸ [x] src/models/payment_method.rs
    [ ] tests/unit/payment_test.rs

✓ Added 1 file(s) to chunk 'models'
```

**Non-interactive:**
```
$ merges add models src/models/payment_method.rs
```

What happens internally:
1. Checks out `feat/payments-v2-chunk-2-models`
2. `git checkout feat/payments-v2 -- src/models/payment_method.rs`
3. `git commit --amend --no-edit`
4. Returns to `feat/payments-v2`
5. Updates `.merges.json`

Idempotent — adding a file already in the chunk is a no-op.

---

### `merges move [--file <path>... --from <chunk> --to <chunk>]`

Realise `src/api/webhooks.rs` and `src/api/handlers.rs` depend on models not yet merged and should ship with the `models` chunk, not `api`.

**Interactive UI (default):**
```
$ merges move
? Move files FROM: api (3 files)
? Files to move from 'api' (Space = toggle, Enter = confirm)
  ▸ [x] src/api/webhooks.rs
    [x] src/api/handlers.rs
    [ ] src/api/routes.rs
? Move 2 file(s) TO: models

✓ Moved 2 file(s) from 'api' → 'models'
```

**Non-interactive:**
```
$ merges move src/api/webhooks.rs src/api/handlers.rs --from api --to models
```

**What happens internally:**
1. **Moves actual file changes**: `merges` doesn't just move references; it modifies the git history of your chunk branches.
2. Soft-resets the source chunk branch, unstages the file, and re-commits.
3. Checks out the file from your source branch into the destination chunk branch and amends its commit.
4. Updates `.merges.json` to reflect the new assignment.

---

### `merges clean [--merged] [-y]`

After PRs are merged:

```
$ merges clean --merged --yes

✓ Deleted feat/payments-v2-chunk-1-db   (PR #101 merged)
✓ Deleted feat/payments-v2-chunk-2-models (PR #102 merged)
· Skipped feat/payments-v2-chunk-3-api  (PR #103 still open)
```

Without `--merged`, it offers to delete all chunk branches regardless of PR state.

---

### `merges doctor [--repair]`

Validates that your local state is consistent and nothing is broken:

```
$ merges doctor

✗ Chunk branch 'feat/payments-v2-chunk-3-api' does not exist locally.
✗ .merges.json is not in .git/info/exclude — it may appear as an untracked file.

Run `merges doctor --repair` to attempt automatic fixes.
```

With `--repair`:

```
$ merges doctor --repair

✓ All checks passed — state is healthy.
```

Checks performed:

| Check | What it verifies |
|---|---|
| Branch existence | Each chunk branch listed in `.merges.json` exists locally |
| Worktrees | If worktree mode is on, each worktree directory is present |
| Gitignore | `.merges.json` is listed in `.git/info/exclude` |
| Duplicate files | No file is assigned to more than one chunk (corruption guard) |

`--repair` will re-add `.merges.json` to `.git/info/exclude` if missing. For missing branches or worktrees, it reports the issue so you can re-run `merges sync` or `merges split`.

---

### `merges completions <shell>`

```bash
merges completions bash >> ~/.bash_completion
merges completions zsh  > ~/.zfunc/_merges
merges completions fish > ~/.config/fish/completions/merges.fish
```

---

## MCP / LLM Integration

`merges mcp` starts a stdio JSON-RPC 2.0 server. Connect Claude, GitHub Copilot, Cursor, or any MCP-compatible client — the agent can then plan and execute the entire split workflow autonomously, from inspecting changed files all the way through opening PRs.

### How it works: two-call split

The agent calls `merges_split` without a plan first to see what files exist, then calls it again with a plan once it has decided the grouping:

```
# Call 1: discover changed files
→ merges_split {}
← { "changed_files": ["db/migrations/...", "src/models/...", ...],
    "instructions": "Call merges_split again with a 'plan' array to apply." }

# Call 2: apply grouping
→ merges_split { "plan": [{"name": "db", "files": [...]}, ...] }
← { "success": true, "chunks_created": 5 }
```

### GitHub Copilot (VS Code)

```json
// .vscode/mcp.json
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

```json
// claude_desktop_config.json
{
  "mcpServers": {
    "merges": {
      "command": "merges",
      "args": ["mcp"]
    }
  }
}
```

### Cursor

```json
// .cursor/mcp.json  (or Cursor > Settings > MCP)
{
  "mcpServers": {
    "merges": {
      "command": "merges",
      "args": ["mcp"]
    }
  }
}
```

### Available MCP tools

| Tool | What it does |
|---|---|
| `merges_init` | Initialise `.merges.json` for the repo |
| `merges_split` | List changed files **or** apply a chunk plan |
| `merges_push` | Push branches and create/update GitHub PRs |
| `merges_sync` | Rebase all chunks onto latest base branch |
| `merges_status` | Return chunk/PR/sync status as structured JSON (includes `behind` count per chunk) |
| `merges_add` | Add files to an existing chunk (amends its branch commit) |
| `merges_move` | Move a file from one chunk to another atomically |
| `merges_clean` | Delete chunk branches; `dry_run:true` returns list without deleting |
| `merges_doctor` | Validate state consistency; `repair:true` auto-fixes issues |

---

## Using merges with a coding agent

When a coding agent (GitHub Copilot, Claude, or any MCP-compatible assistant) produces a large diff, `merges` lets you — or the agent itself — break that diff into a chain of small, reviewable PRs without leaving the conversation.

### Quickstart: agent-driven split

Connect the `merges` MCP server to your agent (see [MCP / LLM Integration](#mcp--llm-integration)), then say something like:

```
I have a large branch feat/PROJ-123-big-feature.
Please inspect the changed files, group them into logical
chunks, push each as a separate PR against main, and give
me links to each PR.
```

The agent will execute this in a handful of tool calls:

1. `merges_init` — reads the git remote, writes `.merges.json`
2. `merges_split {}` — lists all changed files so the agent can plan
3. `merges_split { "plan": [...] }` — creates the chunk branches
4. `merges_push { "strategy": "stacked" }` — opens a PR per chunk

No CLI access required on your end.

---

### GitHub Copilot coding agent (cloud)

The Copilot coding agent runs in a sandboxed environment. Pre-install `merges` via `copilot-setup-steps.yml` so every agent session has it available:

```yaml
# .github/copilot-setup-steps.yml
steps:
  - name: Install merges
    run: cargo install --git https://github.com/alxekb/merges merges
```

Then add instructions to your issue or Copilot prompt:

```
After making your changes, split the diff into reviewable
chunks using merges:

  merges init --commit-prefix <TICKET>
  merges split --auto
  merges push --independent

Keep each chunk under ~300 lines changed. Run
`merges doctor` before and after to verify state is healthy.
```

The agent writes the code, `merges` breaks it into PRs — you only review.

---

### Example: splitting a large agent-generated branch

Suppose the Copilot coding agent created `feat/PROJ-99-auth-overhaul` touching 35 files. Open a chat with the `merges` MCP server connected and say:

```
The branch feat/PROJ-99-auth-overhaul is too big to review.
Split it into these chunks:
  - "migrations" → all files under db/migrations/
  - "models"     → all files under src/models/
  - "api"        → all files under src/api/
  - "frontend"   → all files under frontend/
  - "tests"      → all test files

Push as independent PRs targeting main.
```

The agent resolves this in ~4 MCP tool calls — no manual branching required.

---

### Tips for agent workflows

| Tip | Why it matters |
|-----|---------------|
| Use `merges init --worktrees` | Keeps the working tree stable; the agent won't lose file context between tool calls |
| Pass `--commit-prefix <TICKET>` at init | PR titles and commit messages are uniformly prefixed (e.g. `PROJ-99: add payment models`) |
| Have the agent call `merges_doctor` first | Catches stale state from a previous session before any branches are created |
| Use `merges_clean { "dry_run": true }` | Returns the list of branches that would be deleted — let the agent confirm before executing |
| Prefer `--independent` for unrelated chunks | Allows each PR to merge in any order; use `--stacked` only when chunks depend on each other |

---

## Troubleshooting

### Branch already checked out in a worktree

If you're running `merges` in classic mode (i.e., not using `--worktrees`) and you manually check out a chunk's branch into a separate `git worktree`, `merges` will report an error when trying to operate on that branch:

```
Error: Failed to checkout branch 'JCLARK-97246-poc-chunk-2-lib/data'

Caused by:
  Branch 'JCLARK-97246-poc-chunk-2-lib/data' is already used by a worktree at:
    /Users/you/dev/myrepo/worktrees/JCLARK-97246-poc-chunk-2-lib/data

  Please remove the worktree before running this command:
  git worktree remove /Users/you/dev/myrepo/worktrees/JCLARK-97246-poc-chunk-2-lib/data
```

**Solution:**
If you no longer need the separate worktree, remove it using the `git worktree remove` command provided in the error message.

Alternatively, consider using `merges`' built-in [Worktree mode](#worktree-mode) (`merges init --worktrees`) to avoid this class of issues entirely. This allows `merges` to manage isolated working directories for each chunk, preventing conflicts with manual worktree operations.

---

## Daily workflow

```
Morning: main got new commits overnight
  → merges sync                   # rebase all chunks (rerere handles known conflicts)
  → merges status                 # check CI / review state + Sync column shows ↓ N behind

Reviewer asked for a change in the api chunk
  → git checkout feat/payments-v2-chunk-3-api
  → # make the change
  → git commit -m "fix: address review feedback"
  → merges push                   # re-push and update PR #103

Realised a file is in the wrong chunk
  → merges move src/api/webhooks.rs --from api --to models
  → merges push

Forgot a file
  → merges add models src/models/payment_method.rs
  → merges push

Something feels off — check state consistency
  → merges doctor                 # shows ✓ OK / ✗ issues per check
  → merges doctor --repair        # auto-fix gitignore and config issues

PRs #101 and #102 merged — clean up
  → merges clean --merged --yes
```

---

## Worktree mode

By default `merges` switches branches with `git checkout` during `push` and `sync`. If you want your working tree to **never change** — keeping your editor stable and LSP running — enable worktrees at init time:

```bash
merges init --worktrees --base main
```

Each chunk gets its own directory inside `.git/merges-worktrees/`:

```
.git/merges-worktrees/
  feat-payments-v2-chunk-1-db/       ← always on chunk-1-db branch
  feat-payments-v2-chunk-2-models/   ← always on chunk-2-models branch
  feat-payments-v2-chunk-3-api/      ← always on chunk-3-api branch
```

Your main directory stays on `feat/payments-v2` throughout the entire workflow. All commands — `split`, `push`, `sync`, `add`, `move` — operate inside the worktree directories instead of checking out branches.

**Bonus: parallel sync.** Because worktrees are independent directories, `merges sync` rebases all chunks simultaneously:

```
→ Syncing 5 chunk branch(es) onto 'main' (parallel)
[████████████████████████████████████████] 5/5 done   ← all 5 at once
✓ All chunks are up to date with 'main'.
```

Worktree directories live inside `.git/` so they are never committed, never appear in `git status`, and are removed automatically by `merges clean`.

---

## State file — `.merges.json`

Written by `merges init`, excluded from git via `.git/info/exclude` (a per-repo, local-only gitignore that is never committed). This means the file won't appear in `git status` or your diffs by default. To share chunk definitions with teammates, remove the entry from `.git/info/exclude` and commit the file normally.

```json
{
  "base_branch": "main",
  "source_branch": "feat/payments-v2",
  "repo_owner": "acme",
  "repo_name": "myapp",
  "strategy": "stacked",
  "chunks": [
    {
      "name": "db",
      "branch": "feat/payments-v2-chunk-1-db",
      "files": [
        "db/migrations/001_add_payments.sql",
        "db/migrations/002_add_refunds.sql"
      ],
      "pr_number": 101,
      "pr_url": "https://github.com/acme/myapp/pull/101"
    },
    {
      "name": "models",
      "branch": "feat/payments-v2-chunk-2-models",
      "files": [
        "src/models/payment.rs",
        "src/models/refund.rs"
      ],
      "pr_number": 102,
      "pr_url": "https://github.com/acme/myapp/pull/102"
    }
  ]
}
```

---

## License

MIT
