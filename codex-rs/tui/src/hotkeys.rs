use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

const HOTKEYS_RELATIVE_DIR: &str = "lcodex";
const HOTKEYS_FILE_NAME: &str = "hotkeys.toml";

#[derive(Debug, Clone)]
pub(crate) struct HotkeyLoadResult {
    pub(crate) manager: HotkeyManager,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct HotkeyMatch {
    pub(crate) key: String,
    pub(crate) action: String,
}

#[derive(Debug, Clone)]
pub(crate) struct HookCommand {
    pub(crate) command: String,
    pub(crate) from: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HookRunResult {
    pub(crate) status_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

#[derive(Debug, Clone)]
pub(crate) struct HotkeyManager {
    path: PathBuf,
    bindings: HashMap<KeyChord, String>,
    hooks: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HotkeyControlCommand {
    Help,
    List,
    Reload,
    Reset,
    Bind { key: KeyChord, action: String },
    Unbind { key: KeyChord },
    Hook { action: String, command: String },
    Unhook { action: String },
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct KeyChord {
    code: KeyToken,
    ctrl: bool,
    alt: bool,
    shift: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
enum KeyToken {
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    Backspace,
    Function(u8),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct HotkeysToml {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    bindings: BTreeMap<String, String>,
    #[serde(default)]
    actions: BTreeMap<String, String>,
}

const fn default_version() -> u32 {
    1
}

impl HotkeyManager {
    pub(crate) fn load(codex_home: &Path) -> HotkeyLoadResult {
        let path = hotkeys_file_path(codex_home);
        let mut warnings = Vec::new();

        if !path.exists() {
            return HotkeyLoadResult {
                manager: Self::with_defaults(path),
                warnings,
            };
        }

        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(err) => {
                warnings.push(format!(
                    "failed to read hotkeys config {}: {err}",
                    path.display()
                ));
                return HotkeyLoadResult {
                    manager: Self::with_defaults(path),
                    warnings,
                };
            }
        };

        let parsed = match toml::from_str::<HotkeysToml>(&raw) {
            Ok(parsed) => parsed,
            Err(err) => {
                warnings.push(format!(
                    "failed to parse hotkeys config {}: {err}",
                    path.display()
                ));
                return HotkeyLoadResult {
                    manager: Self::with_defaults(path),
                    warnings,
                };
            }
        };

        let mut bindings = HashMap::new();
        for (raw_key, raw_action) in parsed.bindings {
            let Ok(key) = KeyChord::parse(&raw_key) else {
                warnings.push(format!("ignored invalid hotkey '{raw_key}'"));
                continue;
            };
            let Some(action) = normalize_action_name(&raw_action) else {
                warnings.push(format!(
                    "ignored invalid action '{raw_action}' for key '{raw_key}'"
                ));
                continue;
            };
            bindings.insert(key, action);
        }

        let mut hooks = HashMap::new();
        for (raw_action, command) in parsed.actions {
            let Some(action) = normalize_action_name(&raw_action) else {
                warnings.push(format!("ignored invalid hook action '{raw_action}'"));
                continue;
            };
            let command = command.trim();
            if command.is_empty() {
                warnings.push(format!(
                    "ignored empty hook command for action '{raw_action}'"
                ));
                continue;
            }
            hooks.insert(action, command.to_string());
        }

        HotkeyLoadResult {
            manager: Self {
                path,
                bindings,
                hooks,
            },
            warnings,
        }
    }

    pub(crate) fn save(&self) -> Result<(), String> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| format!("invalid hotkeys path: {}", self.path.display()))?;
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;

        let mut bindings = BTreeMap::new();
        for (key, action) in &self.bindings {
            bindings.insert(key.to_string(), action.clone());
        }

        let mut actions = BTreeMap::new();
        for (action, command) in &self.hooks {
            actions.insert(action.clone(), command.clone());
        }

        let doc = HotkeysToml {
            version: default_version(),
            bindings,
            actions,
        };
        let serialized = toml::to_string_pretty(&doc)
            .map_err(|err| format!("failed to serialize hotkeys config: {err}"))?;
        fs::write(&self.path, serialized)
            .map_err(|err| format!("failed to write {}: {err}", self.path.display()))
    }

    pub(crate) fn reset_to_defaults(&mut self) {
        self.bindings = default_bindings();
        self.hooks.clear();
    }

    pub(crate) fn hotkey_match_for_event(&self, event: KeyEvent) -> Option<HotkeyMatch> {
        let chord = KeyChord::from_key_event(event)?;
        let action = self.bindings.get(&chord)?.clone();
        Some(HotkeyMatch {
            key: chord.to_string(),
            action,
        })
    }

    pub(crate) fn bind(&mut self, key: KeyChord, action: String) {
        self.bindings.insert(key, action);
    }

    pub(crate) fn unbind(&mut self, key: &KeyChord) -> bool {
        self.bindings.remove(key).is_some()
    }

    pub(crate) fn set_hook(&mut self, action: String, command: String) {
        self.hooks.insert(action, command);
    }

    pub(crate) fn remove_hook(&mut self, action: &str) -> bool {
        self.hooks.remove(action).is_some()
    }

    pub(crate) fn resolve_hook_command(&self, action: &str) -> Option<HookCommand> {
        if let Some(command) = self.hooks.get(action) {
            return Some(HookCommand {
                command: command.clone(),
                from: "config",
            });
        }

        if let Some(command) = env_hook_for_action(action) {
            return Some(HookCommand {
                command,
                from: "env",
            });
        }

        None
    }

    pub(crate) fn render_summary(&self) -> String {
        let mut lines = vec![format!("Hotkey config: {}", self.path.display())];

        lines.push("Bindings:".to_string());
        let mut rendered_bindings: Vec<(String, String)> = self
            .bindings
            .iter()
            .map(|(key, action)| (key.to_string(), action.clone()))
            .collect();
        rendered_bindings.sort_by(|a, b| a.0.cmp(&b.0));
        if rendered_bindings.is_empty() {
            lines.push("  (none)".to_string());
        } else {
            for (key, action) in rendered_bindings {
                lines.push(format!("  {key} -> {action}"));
            }
        }

        lines.push("Hooks:".to_string());
        let mut rendered_hooks: Vec<(String, String)> = self
            .hooks
            .iter()
            .map(|(action, command)| (action.clone(), command.clone()))
            .collect();
        rendered_hooks.sort_by(|a, b| a.0.cmp(&b.0));
        if rendered_hooks.is_empty() {
            lines.push("  (none; can also use env: LCODEX_HOTKEY_ACTION_<ACTION>)".to_string());
        } else {
            for (action, command) in rendered_hooks {
                lines.push(format!("  {action} = {command}"));
            }
        }

        lines.join("\n")
    }

    pub(crate) fn help_text() -> &'static str {
        "Hotkey commands:\n  /hotkey\n  /hotkey list\n  /hotkey bind <key> <action>\n  /hotkey unbind <key>\n  /hotkey hook <action> <shell-command>\n  /hotkey unhook <action>\n  /hotkey reload\n  /hotkey reset\nExamples:\n  /hotkey bind ctrl+i takeover\n  /hotkey bind ctrl+u learn\n  /hotkey bind ctrl+o detach\n  /hotkey hook takeover ./scripts/handoff.sh\n  /hotkey hook learn ./scripts/learn.sh"
    }

    fn with_defaults(path: PathBuf) -> Self {
        Self {
            path,
            bindings: default_bindings(),
            hooks: HashMap::new(),
        }
    }
}

impl KeyChord {
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        let normalized = raw.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return Err("key cannot be empty".to_string());
        }

        let parts: Vec<&str> = normalized
            .split('+')
            .filter(|part| !part.is_empty())
            .collect();
        if parts.is_empty() {
            return Err(format!("invalid key: {raw}"));
        }

        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut key_token: Option<KeyToken> = None;

        for part in parts {
            match part {
                "ctrl" | "control" | "ctl" | "c" => ctrl = true,
                "alt" | "opt" | "option" | "a" => alt = true,
                "shift" | "s" => shift = true,
                _ => {
                    if key_token.is_some() {
                        return Err(format!(
                            "invalid key '{raw}': only one key token is allowed"
                        ));
                    }
                    key_token = Some(parse_key_token(part).ok_or_else(|| {
                        format!("unsupported key token '{part}' (example: ctrl+i, ctrl+u, ctrl+o)")
                    })?);
                }
            }
        }

        if !ctrl && !alt && !shift {
            return Err(format!(
                "invalid key '{raw}': at least one modifier is required"
            ));
        }

        let Some(code) = key_token else {
            return Err(format!("invalid key '{raw}': missing key token"));
        };

        Ok(Self {
            code,
            ctrl,
            alt,
            shift,
        })
    }

    pub(crate) fn from_key_event(event: KeyEvent) -> Option<Self> {
        if event.kind != KeyEventKind::Press {
            return None;
        }

        let mods = event.modifiers;
        let ctrl = mods.contains(KeyModifiers::CONTROL);
        let alt = mods.contains(KeyModifiers::ALT);
        let shift = mods.contains(KeyModifiers::SHIFT);

        if !ctrl && !alt && !shift {
            return None;
        }

        let code = match event.code {
            KeyCode::Char(c) => KeyToken::Char(c.to_ascii_lowercase()),
            KeyCode::Enter => KeyToken::Enter,
            KeyCode::Esc => KeyToken::Esc,
            KeyCode::Tab => KeyToken::Tab,
            KeyCode::BackTab => KeyToken::BackTab,
            KeyCode::Left => KeyToken::Left,
            KeyCode::Right => KeyToken::Right,
            KeyCode::Up => KeyToken::Up,
            KeyCode::Down => KeyToken::Down,
            KeyCode::Home => KeyToken::Home,
            KeyCode::End => KeyToken::End,
            KeyCode::PageUp => KeyToken::PageUp,
            KeyCode::PageDown => KeyToken::PageDown,
            KeyCode::Insert => KeyToken::Insert,
            KeyCode::Delete => KeyToken::Delete,
            KeyCode::Backspace => KeyToken::Backspace,
            KeyCode::F(n) => KeyToken::Function(n),
            _ => return None,
        };

        Some(Self {
            code,
            ctrl,
            alt,
            shift,
        })
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if self.ctrl {
            parts.push("ctrl".to_string());
        }
        if self.alt {
            parts.push("alt".to_string());
        }
        if self.shift {
            parts.push("shift".to_string());
        }
        parts.push(self.code.to_string());
        write!(f, "{}", parts.join("+"))
    }
}

impl fmt::Display for KeyToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyToken::Char(' ') => write!(f, "space"),
            KeyToken::Char(c) => write!(f, "{c}"),
            KeyToken::Enter => write!(f, "enter"),
            KeyToken::Esc => write!(f, "esc"),
            KeyToken::Tab => write!(f, "tab"),
            KeyToken::BackTab => write!(f, "backtab"),
            KeyToken::Left => write!(f, "left"),
            KeyToken::Right => write!(f, "right"),
            KeyToken::Up => write!(f, "up"),
            KeyToken::Down => write!(f, "down"),
            KeyToken::Home => write!(f, "home"),
            KeyToken::End => write!(f, "end"),
            KeyToken::PageUp => write!(f, "pgup"),
            KeyToken::PageDown => write!(f, "pgdn"),
            KeyToken::Insert => write!(f, "insert"),
            KeyToken::Delete => write!(f, "delete"),
            KeyToken::Backspace => write!(f, "backspace"),
            KeyToken::Function(n) => write!(f, "f{n}"),
        }
    }
}

pub(crate) fn normalize_action_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lowered = trimmed.to_ascii_lowercase();
    lowered
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        .then_some(lowered)
}

pub(crate) fn parse_control_command(raw: &str) -> Result<HotkeyControlCommand, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("list") {
        return Ok(HotkeyControlCommand::List);
    }
    if trimmed.eq_ignore_ascii_case("help") {
        return Ok(HotkeyControlCommand::Help);
    }
    if trimmed.eq_ignore_ascii_case("reload") {
        return Ok(HotkeyControlCommand::Reload);
    }
    if trimmed.eq_ignore_ascii_case("reset") {
        return Ok(HotkeyControlCommand::Reset);
    }

    if let Some(rest) = trimmed.strip_prefix("bind ") {
        let mut parts = rest.split_whitespace();
        let raw_key = parts
            .next()
            .ok_or_else(|| "usage: /hotkey bind <key> <action>".to_string())?;
        let raw_action = parts
            .next()
            .ok_or_else(|| "usage: /hotkey bind <key> <action>".to_string())?;
        if parts.next().is_some() {
            return Err("usage: /hotkey bind <key> <action>".to_string());
        }
        let key = KeyChord::parse(raw_key)?;
        let action = normalize_action_name(raw_action)
            .ok_or_else(|| format!("invalid action '{raw_action}'"))?;
        return Ok(HotkeyControlCommand::Bind { key, action });
    }

    if let Some(rest) = trimmed.strip_prefix("unbind ") {
        let mut parts = rest.split_whitespace();
        let raw_key = parts
            .next()
            .ok_or_else(|| "usage: /hotkey unbind <key>".to_string())?;
        if parts.next().is_some() {
            return Err("usage: /hotkey unbind <key>".to_string());
        }
        let key = KeyChord::parse(raw_key)?;
        return Ok(HotkeyControlCommand::Unbind { key });
    }

    if let Some(rest) = trimmed.strip_prefix("hook ") {
        let rest = rest.trim();
        let mut first_space = rest.splitn(2, char::is_whitespace);
        let raw_action = first_space
            .next()
            .ok_or_else(|| "usage: /hotkey hook <action> <shell-command>".to_string())?;
        let raw_command = first_space
            .next()
            .ok_or_else(|| "usage: /hotkey hook <action> <shell-command>".to_string())?
            .trim();
        if raw_command.is_empty() {
            return Err("usage: /hotkey hook <action> <shell-command>".to_string());
        }
        let action = normalize_action_name(raw_action)
            .ok_or_else(|| format!("invalid action '{raw_action}'"))?;
        return Ok(HotkeyControlCommand::Hook {
            action,
            command: raw_command.to_string(),
        });
    }

    if let Some(rest) = trimmed.strip_prefix("unhook ") {
        let mut parts = rest.split_whitespace();
        let raw_action = parts
            .next()
            .ok_or_else(|| "usage: /hotkey unhook <action>".to_string())?;
        if parts.next().is_some() {
            return Err("usage: /hotkey unhook <action>".to_string());
        }
        let action = normalize_action_name(raw_action)
            .ok_or_else(|| format!("invalid action '{raw_action}'"))?;
        return Ok(HotkeyControlCommand::Unhook { action });
    }

    Err(format!(
        "unknown /hotkey subcommand: '{trimmed}'. Try '/hotkey help'."
    ))
}

pub(crate) async fn run_hook_command(
    command: &str,
    cwd: &Path,
    env_vars: &[(String, String)],
) -> Result<HookRunResult, String> {
    let mut child = if cfg!(windows) {
        let mut cmd = tokio::process::Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    } else {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-lc").arg(command);
        cmd
    };

    child.current_dir(cwd);
    for (key, value) in env_vars {
        child.env(key, value);
    }

    let output = child
        .output()
        .await
        .map_err(|err| format!("failed to run hook command: {err}"))?;

    Ok(HookRunResult {
        status_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

fn parse_key_token(token: &str) -> Option<KeyToken> {
    match token {
        "enter" => Some(KeyToken::Enter),
        "esc" | "escape" => Some(KeyToken::Esc),
        "tab" => Some(KeyToken::Tab),
        "backtab" | "back-tab" => Some(KeyToken::BackTab),
        "left" => Some(KeyToken::Left),
        "right" => Some(KeyToken::Right),
        "up" => Some(KeyToken::Up),
        "down" => Some(KeyToken::Down),
        "home" => Some(KeyToken::Home),
        "end" => Some(KeyToken::End),
        "pageup" | "pgup" => Some(KeyToken::PageUp),
        "pagedown" | "pgdn" => Some(KeyToken::PageDown),
        "ins" | "insert" => Some(KeyToken::Insert),
        "del" | "delete" => Some(KeyToken::Delete),
        "bs" | "backspace" => Some(KeyToken::Backspace),
        "space" => Some(KeyToken::Char(' ')),
        _ if token.starts_with('f') => {
            let number = token.strip_prefix('f')?.parse::<u8>().ok()?;
            Some(KeyToken::Function(number))
        }
        _ => {
            let mut chars = token.chars();
            let first = chars.next()?;
            chars.next().is_none().then_some(KeyToken::Char(first))
        }
    }
}

fn default_bindings() -> HashMap<KeyChord, String> {
    let mut bindings = HashMap::new();
    bindings.insert(
        KeyChord::parse("ctrl+i").expect("valid default key"),
        "takeover".to_string(),
    );
    bindings.insert(
        KeyChord::parse("ctrl+u").expect("valid default key"),
        "learn".to_string(),
    );
    bindings.insert(
        KeyChord::parse("ctrl+o").expect("valid default key"),
        "detach".to_string(),
    );
    bindings
}

fn hotkeys_file_path(codex_home: &Path) -> PathBuf {
    codex_home
        .join(HOTKEYS_RELATIVE_DIR)
        .join(HOTKEYS_FILE_NAME)
}

fn env_hook_for_action(action: &str) -> Option<String> {
    let dynamic_var = format!(
        "LCODEX_HOTKEY_ACTION_{}",
        action
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() {
                    ch.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect::<String>()
    );
    if let Ok(value) = std::env::var(&dynamic_var)
        && !value.trim().is_empty()
    {
        return Some(value);
    }

    let compat_var = match action {
        "takeover" => Some("LCODEX_TAKEOVER_CMD"),
        "learn" => Some("LCODEX_LEARN_CMD"),
        "detach" => Some("LCODEX_DETACH_CMD"),
        _ => None,
    };

    let Some(var_name) = compat_var else {
        return None;
    };

    let Ok(value) = std::env::var(var_name) else {
        return None;
    };
    (!value.trim().is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;
    use tempfile::tempdir;

    #[test]
    fn key_chord_round_trip() {
        let parsed = KeyChord::parse("ctrl+u").expect("parse ctrl+u");
        assert_eq!(parsed.to_string(), "ctrl+u");
    }

    #[test]
    fn key_chord_requires_modifier() {
        assert!(KeyChord::parse("1").is_err());
    }

    #[test]
    fn parse_control_command_bind() {
        let command = parse_control_command("bind ctrl+1 takeover").expect("valid command");
        assert_eq!(
            command,
            HotkeyControlCommand::Bind {
                key: KeyChord::parse("ctrl+1").unwrap(),
                action: "takeover".to_string(),
            }
        );
    }

    #[test]
    fn parse_control_command_hook_preserves_shell() {
        let command = parse_control_command("hook takeover ./scripts/task.sh --mode handoff")
            .expect("valid command");
        assert_eq!(
            command,
            HotkeyControlCommand::Hook {
                action: "takeover".to_string(),
                command: "./scripts/task.sh --mode handoff".to_string(),
            }
        );
    }

    #[test]
    fn load_defaults_when_missing_file() {
        let dir = tempdir().expect("tempdir");
        let loaded = HotkeyManager::load(dir.path());
        assert!(loaded.warnings.is_empty());
        let event = KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL);
        let matched = loaded
            .manager
            .hotkey_match_for_event(event)
            .expect("default binding should match");
        assert_eq!(matched.action, "takeover");
    }

    #[test]
    fn save_then_load_persists_bindings_and_hooks() {
        let dir = tempdir().expect("tempdir");
        let mut manager = HotkeyManager::load(dir.path()).manager;
        manager.bind(KeyChord::parse("ctrl+8").unwrap(), "triage".to_string());
        manager.set_hook("triage".to_string(), "echo hello".to_string());
        manager.save().expect("save hotkeys");

        let loaded = HotkeyManager::load(dir.path());
        assert!(loaded.warnings.is_empty());
        let event = KeyEvent::new(KeyCode::Char('8'), KeyModifiers::CONTROL);
        let matched = loaded
            .manager
            .hotkey_match_for_event(event)
            .expect("binding should exist");
        assert_eq!(matched.action, "triage");
        let hook = loaded
            .manager
            .resolve_hook_command("triage")
            .expect("hook should exist");
        assert_eq!(hook.command, "echo hello");
    }
}
