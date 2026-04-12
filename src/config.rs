use crate::platform;
use crate::settings::ShortcutSettings;
use anyhow::{anyhow, Result};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
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

    pub fn open_main_hotkey(&self) -> Result<HotKey> {
        parse_hotkey(&self.open_main)
    }

    pub fn open_tasks_hotkey(&self) -> Result<HotKey> {
        parse_hotkey(&self.open_tasks)
    }

    pub fn open_records_hotkey(&self) -> Result<HotKey> {
        parse_hotkey(&self.open_records)
    }

    pub fn entries(&self) -> [(&'static str, &str); 4] {
        [
            ("快捷输入", self.quick_capture.as_str()),
            ("打开主应用", self.open_main.as_str()),
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
        ("打开主应用", config.open_main.as_str()),
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
    if modifiers.contains(Modifiers::META) {
        tokens.push("cmd".to_string());
    }
    if modifiers.contains(Modifiers::CONTROL) {
        tokens.push("ctrl".to_string());
    }
    if modifiers.contains(Modifiers::ALT) {
        tokens.push("alt".to_string());
    }
    if modifiers.contains(Modifiers::SHIFT) {
        tokens.push("shift".to_string());
    }
    tokens.push(code_to_token(code).to_string());
    Ok(tokens.join("+"))
}

fn parse_hotkey_parts(shortcut: &str) -> Result<(Modifiers, Code)> {
    let mut modifiers = Modifiers::empty();
    let mut code = None;

    for token in shortcut.split('+').map(|part| part.trim()) {
        if token.is_empty() {
            continue;
        }

        match token.to_ascii_lowercase().as_str() {
            "cmd" | "command" => modifiers |= Modifiers::META,
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "alt" | "option" => modifiers |= Modifiers::ALT,
            "shift" => modifiers |= Modifiers::SHIFT,
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
