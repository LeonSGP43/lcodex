# lcodex

`lcodex` is an independently managed Codex-based project.

Primary guide:

- `docs/lcodex-development-and-usage.md`

Quick commands:

```bash
# Sync latest upstream main into local main
./scripts/lcodex-sync-upstream.sh

# Build CLI
cd codex-rs
cargo build --release --bin codex
```

Recommended remotes:

- `origin`: your own `lcodex` repository
- `upstream`: `https://github.com/openai/codex.git`
