use crate::platform;
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
        // 暂时返回默认配置，后续可从文件加载
        Self::default()
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

pub fn parse_hotkey(shortcut: &str) -> Result<HotKey> {
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
    let modifiers = if modifiers.is_empty() {
        None
    } else {
        Some(modifiers)
    };

    Ok(HotKey::new(modifiers, code))
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
