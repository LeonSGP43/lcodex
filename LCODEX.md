# lcodex

`lcodex` is Leon's independently managed Codex edition.

## Core docs

1. `README.md`
2. `docs/leon-edition-updates.md`
3. `docs/leon-usage-flow.md`
4. `docs/lcodex-development-and-usage.md`

## Quick commands

```bash
# sync upstream
./scripts/lcodex-sync-upstream.sh

# build release binary
cd codex-rs
cargo build --release --bin codex

# run leon edition
lcodex

# run isolated mode
lcodex -l
```

## Remotes

- `origin`: your `lcodex` repository
- `upstream`: `https://github.com/openai/codex.git`
