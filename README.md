# merges

> Break a large feature branch into small, reviewable PRs — with automatic branch management, GitHub PR creation, and first-class MCP/LLM support.

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

```bash
# From source (requires Rust ≥ 1.75)
git clone https://github.com/alxekb/merges
cd merges
cargo install --path .
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

### `merges init [--base <branch>]`

Initialises `merges` for the current repo. Detects the current branch and GitHub remote automatically.

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

### `merges push [--stacked | --independent]`

```
$ merges push --stacked

→ Pushing 5 chunk(s) as stacked PRs
⠸ [db]       Rebasing onto 'main'…
✓ [db]       PR #101 created → https://github.com/acme/myapp/pull/101
✓ [models]   PR #102 created → https://github.com/acme/myapp/pull/102
✓ [api]      PR #103 created → https://github.com/acme/myapp/pull/103
✓ [frontend] PR #104 created → https://github.com/acme/myapp/pull/104
✓ [tests]    PR #105 created → https://github.com/acme/myapp/pull/105

✓ All chunks pushed successfully!
  Run merges status to see PR status.
```

For each chunk:
1. `git checkout feat/payments-v2-chunk-N-<name>`
2. `git fetch origin && git rebase --update-refs origin/main`  ← `--update-refs` keeps the whole stack aligned
3. `git push origin feat/payments-v2-chunk-N-<name> --force-with-lease`
4. Creates (or updates) a GitHub PR

**Running `merges push` again** (e.g. after adding commits) re-pushes and updates any open PRs. No duplicate PRs are created.

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

╔═══╦══════════╦══════════════════════════════════════╦═════╦═════════╦══════════════════╦═══════╗
║ # ║ Chunk    ║ Branch                               ║ PR  ║ CI      ║ Review           ║ Files ║
╠═══╬══════════╬══════════════════════════════════════╬═════╬═════════╬══════════════════╬═══════╣
║ 1 ║ db       ║ feat/payments-v2-chunk-1-db          ║ #101║ success ║ approved         ║   2   ║
║ 2 ║ models   ║ feat/payments-v2-chunk-2-models      ║ #102║ success ║ approved         ║   2   ║
║ 3 ║ api      ║ feat/payments-v2-chunk-3-api         ║ #103║ pending ║ pending          ║   3   ║
║ 4 ║ frontend ║ feat/payments-v2-chunk-4-frontend    ║ #104║ failure ║ changes_requested║   3   ║
║ 5 ║ tests    ║ feat/payments-v2-chunk-5-tests       ║ #105║ pending ║ pending          ║   2   ║
╚═══╩══════════╩══════════════════════════════════════╩═════╩═════════╩══════════════════╩═══════╝
```

---

### `merges add <chunk> <file>...`

You forgot `src/models/payment_method.rs` and it should be in the `models` chunk:

```
$ merges add models src/models/payment_method.rs

✓ Added 1 file(s) to chunk 'models'
```

What happens internally:
1. Checks out `feat/payments-v2-chunk-2-models`
2. `git checkout feat/payments-v2 -- src/models/payment_method.rs`
3. `git commit --amend --no-edit`
4. Returns to `feat/payments-v2`
5. Updates `.merges.json`

Idempotent — adding a file already in the chunk is a no-op.

---

### `merges move <file> --from <chunk> --to <chunk>`

You realise `src/api/webhooks.rs` depends on models not yet merged and should ship with the `models` chunk, not `api`:

```
$ merges move src/api/webhooks.rs --from api --to models

✓ Moved 'src/api/webhooks.rs' from 'api' → 'models'
```

What happens internally:
1. Checks out `feat/payments-v2-chunk-3-api`, soft-resets, unstages `webhooks.rs`, re-commits the rest
2. Checks out `feat/payments-v2-chunk-2-models`, checks out `webhooks.rs` from source branch, amends commit
3. Returns to `feat/payments-v2`
4. Updates `.merges.json`

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

### `merges completions <shell>`

```bash
merges completions bash >> ~/.bash_completion
merges completions zsh  > ~/.zfunc/_merges
merges completions fish > ~/.config/fish/completions/merges.fish
```

---

## MCP / LLM Integration

`merges mcp` starts a stdio JSON-RPC 2.0 server. Connect Claude, GitHub Copilot, or any MCP-compatible client — the LLM can then plan and execute the entire split workflow autonomously.

### Two-call split workflow

The LLM calls `merges_split` without a plan first to see what files exist, then calls it again with a plan once it has decided the grouping:

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

### Available MCP tools

| Tool | What it does |
|---|---|
| `merges_init` | Initialise `.merges.json` for the repo |
| `merges_split` | List changed files **or** apply a chunk plan |
| `merges_push` | Push branches and create/update GitHub PRs |
| `merges_sync` | Rebase all chunks onto latest base branch |
| `merges_status` | Return chunk/PR status as structured JSON |

---

## Daily workflow

```
Morning: main got new commits overnight
  → merges sync                   # rebase all chunks (rerere handles known conflicts)
  → merges status                 # check CI / review state

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

Written by `merges init`, excluded from git via `.git/info/exclude`. Commit it if you want to share chunk definitions with teammates.

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
