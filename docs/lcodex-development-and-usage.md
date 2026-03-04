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

## Hotkey Control

`lcodex` includes a built-in hotkey manager in TUI mode.

- Defaults:
  - `Ctrl+I -> takeover`
  - `Ctrl+U -> learn`
  - `Ctrl+O -> detach`
- Config file: `~/.codex/lcodex/hotkeys.toml` (or `~/.lcodex/lcodex/hotkeys.toml` in `-l` mode)
- Managed task state file (native takeover flow): `~/.codex/lcodex/managed_task.json`

Manage it with slash commands:

```text
/hotkey
/hotkey list
/hotkey bind <key> <action>
/hotkey unbind <key>
/hotkey hook <action> <shell-command>
/hotkey unhook <action>
/hotkey reload
/hotkey reset
```

By default, these actions are handled natively inside `lcodex` (no external scripts required):

- `takeover`: create a `requiresHuman=true` addnew task for supervisor takeover and sync current rollout to session summary
- `learn`: upload current rollout raw content to `/api/sessions/{id}/summary`
- `detach`: send `pause` signal to the managed task
- `done`: mark managed task as `done`
- while managed: lcodex keeps running locally in TUI and streams new user/assistant messages into task progress events for supervisor monitoring

Hotkey hooks are still supported as an override path. Hook commands receive context env vars:

- `LCODEX_HOTKEY_KEY`
- `LCODEX_HOTKEY_ACTION`
- `LCODEX_CWD`
- `LCODEX_MODEL`
- `LCODEX_THREAD_ID` (if available)
- `LCODEX_THREAD_NAME` (if available)
- `LCODEX_RESUME_COMMAND` (if available)

You can also provide hook commands via env:

- `LCODEX_HOTKEY_ACTION_<ACTION>`
- Compatibility vars: `LCODEX_TAKEOVER_CMD`, `LCODEX_LEARN_CMD`, `LCODEX_DETACH_CMD`

Native integration can be toggled with:

- `LCODEX_NATIVE_BLAZE_ENABLED=true|false` (default: `true`)
- `LCODEX_NATIVE_BLAZE_PREFER_HOOK=true|false` (default: `false`; when `true`, hook commands take precedence over native actions)

## New Device Onboarding

Use this flow when you switch to a new Mac/Linux/Windows machine.

1. Clone and enter repo:

```bash
git clone https://github.com/<you>/lcodex.git
cd lcodex
```

2. Install wrapper command (`lcodex`) into your `PATH`:

```bash
mkdir -p "$HOME/bin"
cat >"$HOME/bin/lcodex" <<'SH'
#!/bin/sh
set -eu
REPO="$HOME/Desktop/LeonProjects/lcodex/codex-rs"
TUI_MANIFEST="$REPO/tui/Cargo.toml"
BIN="$REPO/target/debug/codex-tui"
STAMP="$REPO/target/debug/.lcodex_tui_build_rev"
HEAD="$(git -C "$REPO" rev-parse HEAD 2>/dev/null || true)"
need_build=0
if [ ! -x "$BIN" ]; then
  need_build=1
elif [ -n "$HEAD" ] && { [ ! -f "$STAMP" ] || [ "$(cat "$STAMP" 2>/dev/null || true)" != "$HEAD" ]; }; then
  need_build=1
fi
if [ "$need_build" -eq 1 ]; then
  cargo build --manifest-path "$TUI_MANIFEST" --bin codex-tui
  [ -n "$HEAD" ] && { mkdir -p "$(dirname "$STAMP")"; printf '%s\n' "$HEAD" >"$STAMP"; }
fi
exec "$BIN" "$@"
SH
chmod +x "$HOME/bin/lcodex"
```

3. Configure hotkeys once (file-based, no per-session setup):
Path: `~/.codex/lcodex/hotkeys.toml`

```toml
version = 1

[bindings]
"ctrl+i" = "takeover"
"ctrl+u" = "learn"
"ctrl+o" = "detach"
```

Optional: if you want custom shell behavior, add `[actions]` entries for specific actions.

4. Verify inside TUI:

```text
/hotkey list
```

5. Keep BlazeClaw API envs available on every device:
- `BLAZECLAW_BASE_URL`
- `BLAZECLAW_ADMIN_TOKEN`
- Optional only: advanced tuning vars (`BLAZECLAW_ADDNEW_*`, `BLAZECLAW_LEARN_*`).
  In native lcodex hotkey mode, session identity is auto-derived from `LCODEX_THREAD_ID`.

If your shell uses env-prefix command `c`, run as:

```bash
c lcodex
```

Hotkeys then work directly inside TUI:
- `Ctrl+I`: takeover (supervisor acceptance/interaction handoff; no auto execution claim)
- `Ctrl+U`: learn sync (uploads raw rollout text)
- `Ctrl+O`: detach/unmanage (pause signal)
- Worker identity is auto-tagged with user/host/project/thread, so same-project parallel workers are uniquely identifiable.
- Multiple lcodex workers (different projects/terminals/devices) can mount to one supervisor with the same `BLAZECLAW_BASE_URL` + token.
- When managed, lcodex emits real-time callbacks: TurnComplete -> `callbacks/completed` (task -> `review_pending`); approval/user-input events -> `callbacks/need-human` (task -> `waiting_human`).

To inspect managed worker conversation/status from supervisor:

```bash
# list queue/active
curl "$BLAZECLAW_BASE_URL/api/tasks/queue?limit=50"

# per-task event stream (includes task.callback_progress conversation mirror)
curl "$BLAZECLAW_BASE_URL/api/tasks/<task_id>/events?limit=200"
```

If you customized `~/.codex/lcodex/hotkeys.toml` (for example `Ctrl+I/U/O`), your custom bindings take precedence.

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
