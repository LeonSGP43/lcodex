# lcodex Development and Usage

This document describes the engineering workflow for maintaining Leon's
`lcodex` while continuously inheriting upstream improvements.

## Positioning

`lcodex` is an independent product repository, not a disposable local fork.

- Product evolution happens in `origin` (`lcodex`).
- Upstream sync comes from `openai/codex` via `upstream` remote.

## Remote Strategy

Expected remote layout:

```bash
git remote -v
```

```text
origin   https://github.com/<you>/lcodex.git
upstream https://github.com/openai/codex.git
```

## Build Strategy

### Release build

```bash
cd codex-rs
cargo build --release --bin codex
```

### Debug build (faster iteration)

```bash
cd codex-rs
cargo build --bin codex
```

## Runtime Strategy

Use wrapper command `lcodex` to avoid conflicting with official `codex`.

```bash
lcodex --version
lcodex
lcodex -l
```

- default mode: shared profile (`~/.codex`)
- `-l` mode: isolated profile (`~/.lcodex`)

## Upstream Sync Strategy

Default sync command:

```bash
./scripts/lcodex-sync-upstream.sh
```

Manual fallback:

```bash
git fetch upstream
git checkout main
git merge upstream/main
```

## Branch Strategy

1. `main`: stable Leon baseline
2. `feature/*`: Leon-specific work
3. `pr/*`: upstream-targeted changes

## Contribution Path to Official Codex

```bash
git checkout main
./scripts/lcodex-sync-upstream.sh
git checkout -b pr/<topic>
```

Implement only the upstream-relevant delta, then submit PR to
`openai/codex`.

## Daily Engineering Checklist

```bash
# sync
./scripts/lcodex-sync-upstream.sh

# build
cd codex-rs && cargo build --release --bin codex

# run
lcodex

# isolated test run
lcodex -l
```

## Related Docs

1. `README.md`
2. `docs/leon-edition-updates.md`
3. `docs/leon-usage-flow.md`
4. `LCODEX.md`
