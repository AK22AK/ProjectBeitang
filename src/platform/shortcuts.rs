#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlobalShortcutDefaults {
    pub quick_capture: &'static str,
    pub open_main: &'static str,
    pub open_tasks: &'static str,
    pub open_records: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppShortcutKeystrokes {
    pub quick_add_overlay: &'static str,
    pub search: &'static str,
    pub settings: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickAddHintLabels {
    pub save: String,
    pub open_destination: String,
    pub open_tasks: String,
    pub open_records: String,
    pub draft_protection: String,
}

pub fn primary_modifier_label() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Cmd"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Ctrl"
    }
}

pub fn default_global_shortcuts() -> GlobalShortcutDefaults {
    #[cfg(target_os = "macos")]
    {
        GlobalShortcutDefaults {
            quick_capture: "Cmd+Shift+T",
            open_main: "Cmd+0",
            open_tasks: "Cmd+2",
            open_records: "Cmd+3",
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        GlobalShortcutDefaults {
            quick_capture: "Ctrl+Shift+T",
            open_main: "Ctrl+0",
            open_tasks: "Ctrl+2",
            open_records: "Ctrl+3",
        }
    }
}

pub fn app_shortcut_keystrokes() -> AppShortcutKeystrokes {
    #[cfg(target_os = "macos")]
    {
        AppShortcutKeystrokes {
            quick_add_overlay: "cmd-n",
            search: "cmd-k",
            settings: "cmd-,",
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        AppShortcutKeystrokes {
            quick_add_overlay: "ctrl-n",
            search: "ctrl-k",
            settings: "ctrl-,",
        }
    }
}

pub fn app_shortcut_entries() -> [(&'static str, &'static str); 3] {
    let modifier = primary_modifier_label();
    match modifier {
        "Cmd" => [("快速创建", "Cmd+N"), ("搜索", "Cmd+K"), ("设置", "Cmd+,")],
        _ => [
            ("快速创建", "Ctrl+N"),
            ("搜索", "Ctrl+K"),
            ("设置", "Ctrl+,"),
        ],
    }
}

pub fn app_shortcuts_intro() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "应用内快捷键由系统菜单统一管理，全局快捷键继续用于跨应用唤起。"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "应用内快捷键只在 Robinne 前台时生效，全局快捷键继续用于跨应用唤起。"
    }
}

pub fn app_shortcut_scope_description() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "仅在 Robinne 前台时生效，并显示在系统菜单中。"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "仅在 Robinne 前台时生效。"
    }
}

pub fn global_shortcut_scope_description() -> &'static str {
    "即使应用未聚焦也可触发，后续版本会在这里补充自定义编辑能力。"
}

pub fn quick_add_placeholder(is_task: bool) -> String {
    let modifier = primary_modifier_label();
    if is_task {
        format!(
            "输入任务标题，Enter 换行添加正文 ({}+Enter 保存, Shift+{}+Enter 打开任务, #标签 @人物)",
            modifier, modifier
        )
    } else {
        format!(
            "输入记录，Enter 换行后首行作为标题 ({}+Enter 保存, Shift+{}+Enter 打开记录, #标签 @人物)",
            modifier, modifier
        )
    }
}

pub fn quick_add_hint_labels() -> QuickAddHintLabels {
    let modifier = primary_modifier_label();
    QuickAddHintLabels {
        save: format!("{modifier}+Enter 保存"),
        open_destination: format!("Shift+{modifier}+Enter 打开对应面板"),
        open_tasks: format!("{modifier}+2 查看任务"),
        open_records: format!("{modifier}+3 查看记录"),
        draft_protection: quick_add_draft_protection_message(),
    }
}

pub fn quick_add_draft_protection_message() -> String {
    let modifier = primary_modifier_label();
    format!("已有草稿，按 Esc 关闭或按 {modifier}+2 / {modifier}+3 查看对应面板")
}
