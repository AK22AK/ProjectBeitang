use crate::ui::sidebar::Panel;

pub const SEARCH_KEYSTROKE: &str = "cmd-k";
pub const SETTINGS_KEYSTROKE: &str = "cmd-,";

pub fn app_shortcut_entries() -> [(&'static str, &'static str); 2] {
    [("搜索", "Cmd+K"), ("设置", "Cmd+,")]
}

pub fn main_panel_shortcuts() -> [(&'static str, Panel); 5] {
    [
        ("1", Panel::Dashboard),
        ("2", Panel::Tasks),
        ("3", Panel::Records),
        ("4", Panel::Timeline),
        ("5", Panel::AI),
    ]
}
