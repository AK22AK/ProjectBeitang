use crate::platform;
use crate::ui::sidebar::Panel;

pub fn quick_add_overlay_keystroke() -> &'static str {
    platform::app_shortcut_keystrokes().quick_add_overlay
}

pub fn search_keystroke() -> &'static str {
    platform::app_shortcut_keystrokes().search
}

pub fn settings_keystroke() -> &'static str {
    platform::app_shortcut_keystrokes().settings
}

pub fn app_shortcut_entries() -> [(&'static str, &'static str); 3] {
    platform::app_shortcut_entries()
}

pub fn main_panel_shortcuts() -> [(&'static str, Panel); 3] {
    [
        ("1", Panel::Dashboard),
        ("4", Panel::Timeline),
        ("5", Panel::AI),
    ]
}
