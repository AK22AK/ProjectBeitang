use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutConfig {
    pub quick_add_task: String,
    pub quick_add_note: String,
    pub view_tasks: String,
    pub view_notes: String,
    pub open_main: String,
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        Self {
            quick_add_task: "Cmd+N".to_string(),
            quick_add_note: "Cmd+M".to_string(),
            view_tasks: "Cmd+1".to_string(),
            view_notes: "Cmd+2".to_string(),
            open_main: "Cmd+0".to_string(),
        }
    }
}

impl ShortcutConfig {
    pub fn load() -> Self {
        // 暂时返回默认配置，后续可从文件加载
        Self::default()
    }
}
