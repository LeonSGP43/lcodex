# Leon Usage Flow and Commands

This is the end-to-end operational guide for `lcodex`.

## 0) One-time setup

### Create/verify remotes

```bash
# inside lcodex repo
git remote -v

# expected
# origin   https://github.com/<you>/lcodex.git
# upstream https://github.com/openai/codex.git
```

If missing:

```bash
git remote add origin https://github.com/<you>/lcodex.git
git remote add upstream https://github.com/openai/codex.git
```

## 1) Daily start routine

```bash
cd /path/to/lcodex
./scripts/lcodex-sync-upstream.sh
```

If fast-forward fails:

```bash
git fetch upstream
git checkout main
git merge upstream/main
```

## 2) Build commands

### Release build (recommended for daily runtime)

```bash
cd codex-rs
cargo build --release --bin codex
```

### Faster debug build (for rapid iteration)

```bash
cd codex-rs
cargo build --bin codex
```

## 3) Run commands

### Shared profile mode (default)

```bash
lccodex
```

Uses the default codex home (`~/.codex`).

### Isolated profile mode

```bash
lccodex -l
```

Uses dedicated Leon profile home (for example `~/.lcodex`).

## 4) Update and rebuild flow

```bash
# sync first
./scripts/lcodex-sync-upstream.sh

# rebuild
cd codex-rs && cargo build --release --bin codex

# verify
lccodex --version
```

## 5) Feature development flow

```bash
git checkout main
./scripts/lcodex-sync-upstream.sh
git checkout -b feature/<topic>
```

Implement -> test -> commit -> push -> open PR to your own `lcodex` repo.

## 6) Upstream contribution flow

```bash
git checkout main
./scripts/lcodex-sync-upstream.sh
git checkout -b pr/<topic>
```

Keep the change scoped and upstream-friendly, then open PR against
`openai/codex`.

## 7) Suggested command checklist

```bash
# status
git status

# sync
./scripts/lcodex-sync-upstream.sh

# build
cd codex-rs && cargo build --release --bin codex

# run
lccodex

# isolated run
lccodex -l
```

## 8) Common troubleshooting

### Build takes very long

Release build can be slow for first compile. Use debug build for rapid local
iteration, then release build when ready.

### Command conflict with system codex

Use `lccodex` wrapper only. Do not overwrite `/opt/homebrew/bin/codex`.

### Want a clean test profile

Use isolated mode:

```bash
lccodex -l
```
