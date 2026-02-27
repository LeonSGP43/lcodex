# lcodex (Leon Edition)

`lcodex` is Leon's independently maintained edition of Codex.

This repo keeps two goals in balance:

1. Run fast and safely for daily personal/team use.
2. Stay syncable with `openai/codex` so upstream improvements can be adopted.

## What Changed In Leon Edition

Compared with the upstream defaults, Leon edition adds a practical local workflow:

1. Independent repo strategy (`origin` for `lcodex`, `upstream` for official Codex).
2. Local command namespace (`lcodex`) to avoid conflicts with system `codex`.
3. Dual runtime modes:
- default mode shares `~/.codex`.
- isolated mode (`-l`) uses separate home (for example `~/.lcodex`).
4. Upstream sync script for daily maintenance.
5. Leon-focused docs for setup, update flow, and contribution strategy.

## Why This Is Better

1. No command conflict: official `codex` and local `lcodex` can coexist.
2. Lower risk: isolated mode allows safe experiments without polluting daily state.
3. Faster operation: standardized commands reduce setup friction.
4. Sustainable maintenance: upstream sync workflow avoids long-term divergence.
5. Better contribution path: you can still open clean PRs to `openai/codex`.

## Quick Start

### 1) Build from source

```bash
cd codex-rs
cargo build --release --bin codex
```

Binary path:

```text
codex-rs/target/release/codex
```

### 2) Recommended wrapper command

Use a shell wrapper (`lcodex`) instead of replacing system `codex`.

Example usage:

```bash
lcodex --version
lcodex
lcodex -l
```

### 3) Sync upstream regularly

```bash
./scripts/lcodex-sync-upstream.sh
```

## Daily Workflow Commands

```bash
# sync official updates
./scripts/lcodex-sync-upstream.sh

# rebuild local binary
cd codex-rs && cargo build --release --bin codex

# run leon edition (shared config)
lcodex

# run leon edition (isolated config)
lcodex -l
```

## Docs Index

1. [Leon Edition Updates and Advantages](./docs/leon-edition-updates.md)
2. [Leon Usage Flow and Commands](./docs/leon-usage-flow.md)
3. [lcodex Development and Usage](./docs/lcodex-development-and-usage.md)
4. [lcodex Repository Note](./LCODEX.md)
5. [Contributing](./docs/contributing.md)

## Upstream PR Strategy

Use two branch categories:

1. `feature/*` for Leon-only product evolution.
2. `pr/*` for changes intended to be proposed upstream.

Recommended flow:

```bash
git checkout main
./scripts/lcodex-sync-upstream.sh
git checkout -b pr/<topic>
```

Then implement, test, and submit a PR to `openai/codex`.

## License

This repository follows the same license model as the base project.
See [LICENSE](./LICENSE).
