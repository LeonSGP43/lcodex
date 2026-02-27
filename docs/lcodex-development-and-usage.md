# lcodex Development and Usage

This document defines the recommended workflow for running `lcodex` as an
independent project while still syncing upstream changes from
`openai/codex`.

## Repository Model

- `origin`: your own `lcodex` repository
- `upstream`: `https://github.com/openai/codex.git`

Current local setup already includes:

- `upstream` -> `https://github.com/openai/codex.git`

Set your own `origin` after creating your `lcodex` remote:

```bash
git remote add origin <your-lcodex-repo-url>
git push -u origin main
```

## Local Build

From repo root:

```bash
cd codex-rs
cargo build --release --bin codex
```

Binary path:

```text
codex-rs/target/release/codex
```

## Recommended Shell Wrapper

Use a wrapper command (for example `lccodex`) to avoid conflicts with system
`codex`:

```bash
lccodex --version
lccodex
lccodex -l
```

- default mode: shares `~/.codex`
- `-l` mode: isolated home (for example `~/.lcodex`)

## Upstream Sync Workflow

Use the helper script:

```bash
./scripts/lcodex-sync-upstream.sh
```

What it does:

1. fetch from `upstream`
2. checkout local `main`
3. fast-forward merge from `upstream/main`

If fast-forward is not possible, resolve manually:

```bash
git fetch upstream
git checkout main
git merge upstream/main
```

## PR Workflow to Official Codex

Keep official contributions isolated in dedicated branches:

```bash
git checkout main
./scripts/lcodex-sync-upstream.sh
git checkout -b pr/<topic>
```

Then implement changes, run tests, and open PR from your branch to
`openai/codex`.

## Daily Workflow

1. Sync upstream regularly.
2. Build and run locally via `lccodex`.
3. Push product-specific features to your own `origin`.
4. For upstream-worthy fixes, use `pr/*` branches.
