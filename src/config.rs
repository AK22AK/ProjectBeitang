use crate::platform;
use crate::settings::ShortcutSettings;
use anyhow::{anyhow, Result};
use global_hotkey::hotkey::{Code, HotKey, Modifiers as HotkeyModifiers};
use gpui::{Keystroke, Modifiers};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutConfig {
    pub quick_capture: String,
    pub open_main: String,
    pub open_tasks: String,
    pub open_records: String,
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        let defaults = platform::default_global_shortcuts();
        Self {
            quick_capture: defaults.quick_capture.to_string(),
            open_main: defaults.open_main.to_string(),
            open_tasks: defaults.open_tasks.to_string(),
            open_records: defaults.open_records.to_string(),
        }
    }
}

impl ShortcutConfig {
    pub fn load() -> Self {
        crate::settings::load_app_settings()
            .map(|settings| Self::from(&settings.shortcuts))
            .unwrap_or_default()
    }

    pub fn quick_capture_hotkey(&self) -> Result<HotKey> {
        parse_hotkey(&self.quick_capture)
    }

    pub fn entries(&self) -> [(&'static str, &str); 4] {
        [
            ("快捷输入", self.quick_capture.as_str()),
            ("看板面板", self.open_main.as_str()),
            ("任务面板", self.open_tasks.as_str()),
            ("记录面板", self.open_records.as_str()),
        ]
    }
}

impl From<&ShortcutSettings> for ShortcutConfig {
    fn from(value: &ShortcutSettings) -> Self {
        Self {
            quick_capture: value.quick_capture.clone(),
            open_main: value.open_main.clone(),
            open_tasks: value.open_tasks.clone(),
            open_records: value.open_records.clone(),
        }
    }
}

impl From<&ShortcutConfig> for ShortcutSettings {
    fn from(value: &ShortcutConfig) -> Self {
        Self {
            quick_capture: value.quick_capture.clone(),
            open_main: value.open_main.clone(),
            open_tasks: value.open_tasks.clone(),
            open_records: value.open_records.clone(),
        }
    }
}

pub fn parse_hotkey(shortcut: &str) -> Result<HotKey> {
    let (modifiers, code) = parse_hotkey_parts(shortcut)?;
    let modifiers = if modifiers.is_empty() {
        None
    } else {
        Some(modifiers)
    };

    Ok(HotKey::new(modifiers, code))
}

pub fn validate_shortcut_config(config: &ShortcutConfig) -> Result<()> {
    let shortcuts = [
        ("快捷输入", config.quick_capture.as_str()),
        ("看板面板", config.open_main.as_str()),
        ("任务面板", config.open_tasks.as_str()),
        ("记录面板", config.open_records.as_str()),
    ];
    let mut normalized = Vec::with_capacity(shortcuts.len());

    for (label, shortcut) in shortcuts {
        parse_hotkey(shortcut)?;
        let normalized_shortcut = normalize_hotkey(shortcut)?;
        if normalized
            .iter()
            .any(|existing| existing == &normalized_shortcut)
        {
            return Err(anyhow!("快捷键 `{label}` 与其他设置重复"));
        }
        normalized.push(normalized_shortcut);
    }

    Ok(())
}

pub fn normalize_hotkey(shortcut: &str) -> Result<String> {
    let (modifiers, code) = parse_hotkey_parts(shortcut)?;
    let mut tokens = Vec::new();
    if modifiers.contains(HotkeyModifiers::META) {
        tokens.push("cmd".to_string());
    }
    if modifiers.contains(HotkeyModifiers::CONTROL) {
        tokens.push("ctrl".to_string());
    }
    if modifiers.contains(HotkeyModifiers::ALT) {
        tokens.push("alt".to_string());
    }
    if modifiers.contains(HotkeyModifiers::SHIFT) {
        tokens.push("shift".to_string());
    }
    tokens.push(code_to_token(code).to_string());
    Ok(tokens.join("+"))
}

pub fn format_shortcut_for_display(shortcut: &str) -> Result<String> {
    let (modifiers, code) = parse_hotkey_parts(shortcut)?;
    let mut tokens = Vec::new();
    if modifiers.contains(HotkeyModifiers::META) {
        #[cfg(target_os = "macos")]
        tokens.push("Cmd".to_string());

        #[cfg(target_os = "windows")]
        tokens.push("Win".to_string());

        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        tokens.push("Super".to_string());
    }
    if modifiers.contains(HotkeyModifiers::CONTROL) {
        tokens.push("Ctrl".to_string());
    }
    if modifiers.contains(HotkeyModifiers::ALT) {
        #[cfg(target_os = "macos")]
        tokens.push("Option".to_string());

        #[cfg(not(target_os = "macos"))]
        tokens.push("Alt".to_string());
    }
    if modifiers.contains(HotkeyModifiers::SHIFT) {
        tokens.push("Shift".to_string());
    }
    tokens.push(code_to_display_token(code).to_string());
    Ok(tokens.join("+"))
}

pub fn preview_shortcut_from_keystroke(keystroke: &Keystroke) -> Result<String> {
    let mut tokens = modifier_preview_tokens(keystroke.modifiers);
    if let Some(key_preview) = preview_key_from_keystroke(&keystroke.key) {
        tokens.push(key_preview);
    }

    if tokens.is_empty() {
        return Err(anyhow!("请按下新的快捷键组合"));
    }

    Ok(tokens.join("+"))
}

pub fn preview_shortcut_from_modifiers(modifiers: Modifiers) -> Option<String> {
    let tokens = modifier_preview_tokens(modifiers);
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join("+"))
    }
}

pub fn shortcut_from_keystroke(keystroke: &Keystroke) -> Result<Option<String>> {
    if keystroke.modifiers.function {
        return Err(anyhow!("暂不支持将 Fn 组合设为快捷键"));
    }

    if is_modifier_key(&keystroke.key) {
        return Ok(None);
    }

    if !keystroke.modifiers.modified() {
        return Err(anyhow!("快捷键必须至少包含一个修饰键"));
    }

    let mut tokens = Vec::new();
    if keystroke.modifiers.platform {
        #[cfg(target_os = "macos")]
        tokens.push("Cmd".to_string());

        #[cfg(target_os = "windows")]
        tokens.push("Win".to_string());

        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        tokens.push("Super".to_string());
    }
    if keystroke.modifiers.control {
        tokens.push("Ctrl".to_string());
    }
    if keystroke.modifiers.alt {
        #[cfg(target_os = "macos")]
        tokens.push("Option".to_string());

        #[cfg(not(target_os = "macos"))]
        tokens.push("Alt".to_string());
    }
    if keystroke.modifiers.shift {
        tokens.push("Shift".to_string());
    }
    tokens.push(display_key_from_keystroke(&keystroke.key)?);
    Ok(Some(tokens.join("+")))
}

pub fn keystroke_matches_shortcut(keystroke: &Keystroke, shortcut: &str) -> bool {
    let Ok(Some(recorded_shortcut)) = shortcut_from_keystroke(keystroke) else {
        return false;
    };
    let Ok(recorded_shortcut) = normalize_hotkey(&recorded_shortcut) else {
        return false;
    };
    let Ok(expected_shortcut) = normalize_hotkey(shortcut) else {
        return false;
    };
    recorded_shortcut == expected_shortcut
}

fn parse_hotkey_parts(shortcut: &str) -> Result<(HotkeyModifiers, Code)> {
    let mut modifiers = HotkeyModifiers::empty();
    let mut code = None;

    for token in shortcut.split('+').map(|part| part.trim()) {
        if token.is_empty() {
            continue;
        }

        match token.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "win" | "super" => modifiers |= HotkeyModifiers::META,
            "ctrl" | "control" => modifiers |= HotkeyModifiers::CONTROL,
            "alt" | "option" => modifiers |= HotkeyModifiers::ALT,
            "shift" => modifiers |= HotkeyModifiers::SHIFT,
            key => {
                if code.is_some() {
                    return Err(anyhow!("快捷键 `{shortcut}` 包含多个主键"));
                }
                code = Some(parse_key_code(key)?);
            }
        }
    }

    let code = code.ok_or_else(|| anyhow!("快捷键 `{shortcut}` 缺少主键"))?;
    Ok((modifiers, code))
}

fn parse_key_code(key: &str) -> Result<Code> {
    if key.len() == 1 {
        let ch = key.chars().next().unwrap().to_ascii_uppercase();
        return match ch {
            '0' => Ok(Code::Digit0),
            '1' => Ok(Code::Digit1),
            '2' => Ok(Code::Digit2),
            '3' => Ok(Code::Digit3),
            '4' => Ok(Code::Digit4),
            '5' => Ok(Code::Digit5),
            '6' => Ok(Code::Digit6),
            '7' => Ok(Code::Digit7),
            '8' => Ok(Code::Digit8),
            '9' => Ok(Code::Digit9),
            'A' => Ok(Code::KeyA),
            'B' => Ok(Code::KeyB),
            'C' => Ok(Code::KeyC),
            'D' => Ok(Code::KeyD),
            'E' => Ok(Code::KeyE),
            'F' => Ok(Code::KeyF),
            'G' => Ok(Code::KeyG),
            'H' => Ok(Code::KeyH),
            'I' => Ok(Code::KeyI),
            'J' => Ok(Code::KeyJ),
            'K' => Ok(Code::KeyK),
            'L' => Ok(Code::KeyL),
            'M' => Ok(Code::KeyM),
            'N' => Ok(Code::KeyN),
            'O' => Ok(Code::KeyO),
            'P' => Ok(Code::KeyP),
            'Q' => Ok(Code::KeyQ),
            'R' => Ok(Code::KeyR),
            'S' => Ok(Code::KeyS),
            'T' => Ok(Code::KeyT),
            'U' => Ok(Code::KeyU),
            'V' => Ok(Code::KeyV),
            'W' => Ok(Code::KeyW),
            'X' => Ok(Code::KeyX),
            'Y' => Ok(Code::KeyY),
            'Z' => Ok(Code::KeyZ),
            _ => Err(anyhow!("不支持的快捷键主键 `{key}`")),
        };
    }

    match key {
        "enter" | "return" => Ok(Code::Enter),
        "esc" | "escape" => Ok(Code::Escape),
        "tab" => Ok(Code::Tab),
        _ => Err(anyhow!("不支持的快捷键主键 `{key}`")),
    }
}

fn code_to_token(code: Code) -> &'static str {
    match code {
        Code::Digit0 => "0",
        Code::Digit1 => "1",
        Code::Digit2 => "2",
        Code::Digit3 => "3",
        Code::Digit4 => "4",
        Code::Digit5 => "5",
        Code::Digit6 => "6",
        Code::Digit7 => "7",
        Code::Digit8 => "8",
        Code::Digit9 => "9",
        Code::KeyA => "a",
        Code::KeyB => "b",
        Code::KeyC => "c",
        Code::KeyD => "d",
        Code::KeyE => "e",
        Code::KeyF => "f",
        Code::KeyG => "g",
        Code::KeyH => "h",
        Code::KeyI => "i",
        Code::KeyJ => "j",
        Code::KeyK => "k",
        Code::KeyL => "l",
        Code::KeyM => "m",
        Code::KeyN => "n",
        Code::KeyO => "o",
        Code::KeyP => "p",
        Code::KeyQ => "q",
        Code::KeyR => "r",
        Code::KeyS => "s",
        Code::KeyT => "t",
        Code::KeyU => "u",
        Code::KeyV => "v",
        Code::KeyW => "w",
        Code::KeyX => "x",
        Code::KeyY => "y",
        Code::KeyZ => "z",
        Code::Enter => "enter",
        Code::Escape => "esc",
        Code::Tab => "tab",
        _ => "unsupported",
    }
}

fn code_to_display_token(code: Code) -> &'static str {
    match code {
        Code::KeyA => "A",
        Code::KeyB => "B",
        Code::KeyC => "C",
        Code::KeyD => "D",
        Code::KeyE => "E",
        Code::KeyF => "F",
        Code::KeyG => "G",
        Code::KeyH => "H",
        Code::KeyI => "I",
        Code::KeyJ => "J",
        Code::KeyK => "K",
        Code::KeyL => "L",
        Code::KeyM => "M",
        Code::KeyN => "N",
        Code::KeyO => "O",
        Code::KeyP => "P",
        Code::KeyQ => "Q",
        Code::KeyR => "R",
        Code::KeyS => "S",
        Code::KeyT => "T",
        Code::KeyU => "U",
        Code::KeyV => "V",
        Code::KeyW => "W",
        Code::KeyX => "X",
        Code::KeyY => "Y",
        Code::KeyZ => "Z",
        Code::Enter => "Enter",
        Code::Escape => "Esc",
        Code::Tab => "Tab",
        _ => code_to_token(code),
    }
}

fn is_modifier_key(key: &str) -> bool {
    matches!(
        key,
        "control" | "alt" | "shift" | "platform" | "command" | "cmd" | "super" | "win" | "fn"
    )
}

fn display_key_from_keystroke(key: &str) -> Result<String> {
    if is_modifier_key(key) {
        return Err(anyhow!("请继续按下主键完成快捷键录制"));
    }

    if key.len() == 1 {
        return Ok(key.to_ascii_uppercase());
    }

    match key {
        "enter" | "return" => Ok("Enter".to_string()),
        "escape" | "esc" => Ok("Esc".to_string()),
        "tab" => Ok("Tab".to_string()),
        _ => Err(anyhow!("不支持将 `{key}` 设为快捷键")),
    }
}

fn modifier_preview_tokens(modifiers: Modifiers) -> Vec<String> {
    let mut tokens = Vec::new();
    if modifiers.platform {
        #[cfg(target_os = "macos")]
        tokens.push("Cmd".to_string());

        #[cfg(target_os = "windows")]
        tokens.push("Win".to_string());

        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        tokens.push("Super".to_string());
    }
    if modifiers.control {
        tokens.push("Ctrl".to_string());
    }
    if modifiers.alt {
        #[cfg(target_os = "macos")]
        tokens.push("Option".to_string());

        #[cfg(not(target_os = "macos"))]
        tokens.push("Alt".to_string());
    }
    if modifiers.shift {
        tokens.push("Shift".to_string());
    }
    if modifiers.function {
        tokens.push("Fn".to_string());
    }
    tokens
}

fn preview_key_from_keystroke(key: &str) -> Option<String> {
    if key.is_empty() || is_modifier_key(key) {
        return None;
    }

    if key.len() == 1 {
        return Some(key.to_ascii_uppercase());
    }

    Some(match key {
        "enter" | "return" => "Enter".to_string(),
        "escape" | "esc" => "Esc".to_string(),
        "tab" => "Tab".to_string(),
        _ => key.to_string(),
    })
}
