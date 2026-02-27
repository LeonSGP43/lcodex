# Leon Edition: Updates and Advantages

This document explains what was changed in `lcodex` (Leon edition), why those
changes matter, and how they improve long-term maintainability.

## Scope

Leon edition is not a temporary local patch. It is a maintained product branch
with explicit repo strategy, command strategy, and documentation strategy.

## Update Summary

### 1) Repository strategy upgraded

From "single fork mindset" to "independent product repo + upstream sync".

- `origin`: Leon's `lcodex` repository.
- `upstream`: `https://github.com/openai/codex.git`.

### 2) Runtime command strategy upgraded

From directly using `codex` to a dedicated command namespace:

- `lcodex`: Leon local edition runtime command.

This avoids overriding or breaking system-installed official `codex`.

### 3) Config isolation strategy upgraded

Two operational modes are clearly defined:

1. Shared mode (default): uses `~/.codex`.
2. Isolated mode (`-l`): uses dedicated home (for example `~/.lcodex`).

### 4) Maintenance workflow upgraded

Added script:

- `scripts/lcodex-sync-upstream.sh`

Purpose:

1. Fetch upstream changes.
2. Move local `main` forward safely (fast-forward).

### 5) Documentation system upgraded

A Leon-focused doc set was added for:

- setup
- daily commands
- update workflow
- upstream PR path

## Advantages

### A) Safer day-to-day use

You can run local experimental builds without impacting your system command
layout.

### B) Better debugging and rollback control

Isolated mode helps reproduce issues with a clean config boundary.

### C) Lower merge cost over time

Explicit upstream sync process reduces drift and conflict debt.

### D) Better team onboarding

New teammates can follow one command flow instead of reconstructing local
knowledge.

### E) Better upstream contribution quality

`pr/*` branches remain clean and reviewable for official PRs.

## Branching Recommendation

1. `main`: stable Leon baseline.
2. `feature/*`: Leon-specific features.
3. `pr/*`: upstream-targeted fixes.

## Minimal Operational Rules

1. Sync `main` from upstream before major feature work.
2. Keep Leon-only and upstream-targeted changes in separate branches.
3. Document user-visible behavior changes immediately in `docs/`.

## Command References

```bash
# sync upstream
./scripts/lcodex-sync-upstream.sh

# build
cd codex-rs && cargo build --release --bin codex

# run shared config
lcodex

# run isolated config
lcodex -l
```
