use crate::ai::AiSettings;
use crate::data_management::app_data_dir;
use crate::data_management::ImportMode;
use crate::git_sync::GitRemoteSyncConfig;
use crate::platform::default_global_shortcuts;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const SETTINGS_FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum StartupPanelPreference {
    #[default]
    Dashboard,
    Tasks,
    Records,
    Timeline,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum QuickAddDefaultMode {
    Task,
    #[default]
    Record,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ImportModePreference {
    #[default]
    ReplaceWithBackup,
    Merge,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneralSettings {
    #[serde(default)]
    pub startup_panel: StartupPanelPreference,
    #[serde(default)]
    pub quick_add_default_mode: QuickAddDefaultMode,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            startup_panel: StartupPanelPreference::Dashboard,
            quick_add_default_mode: QuickAddDefaultMode::Record,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReminderSettings {
    #[serde(default = "default_notifications_enabled")]
    pub notifications_enabled: bool,
}

impl Default for ReminderSettings {
    fn default() -> Self {
        Self {
            notifications_enabled: default_notifications_enabled(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShortcutSettings {
    pub quick_capture: String,
    pub open_main: String,
    pub open_tasks: String,
    pub open_records: String,
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        let defaults = default_global_shortcuts();
        Self {
            quick_capture: defaults.quick_capture.to_string(),
            open_main: defaults.open_main.to_string(),
            open_tasks: defaults.open_tasks.to_string(),
            open_records: defaults.open_records.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataSettings {
    #[serde(default)]
    pub default_import_mode: ImportModePreference,
}

impl Default for DataSettings {
    fn default() -> Self {
        Self {
            default_import_mode: ImportModePreference::ReplaceWithBackup,
        }
    }
}

impl ImportModePreference {
    pub fn to_import_mode(self) -> ImportMode {
        match self {
            Self::ReplaceWithBackup => ImportMode::ReplaceWithBackup,
            Self::Merge => ImportMode::Merge,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    #[serde(default)]
    pub general: GeneralSettings,
    #[serde(default)]
    pub ai: AiSettings,
    #[serde(default)]
    pub reminders: ReminderSettings,
    #[serde(default)]
    pub shortcuts: ShortcutSettings,
    #[serde(default)]
    pub data: DataSettings,
    #[serde(default)]
    pub git_sync: GitRemoteSyncConfig,
}

fn default_notifications_enabled() -> bool {
    true
}

pub fn settings_file_path() -> PathBuf {
    app_data_dir().join(SETTINGS_FILE_NAME)
}

pub fn load_app_settings() -> Result<AppSettings, String> {
    let path = settings_file_path();
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let bytes =
        fs::read(&path).map_err(|err| format!("读取设置文件失败 {}: {}", path.display(), err))?;
    serde_json::from_slice(&bytes)
        .map_err(|err| format!("解析设置文件失败 {}: {}", path.display(), err))
}

pub fn save_app_settings(settings: &AppSettings) -> Result<(), String> {
    let path = settings_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("创建设置目录失败 {}: {}", parent.display(), err))?;
    }

    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|err| format!("序列化设置文件失败 {}: {}", path.display(), err))?;
    fs::write(&path, bytes).map_err(|err| format!("写入设置文件失败 {}: {}", path.display(), err))
}

#[cfg(test)]
mod tests {
    use super::{AppSettings, ImportModePreference};
    use crate::ai::AiProviderProtocol;
    use std::fs;

    #[test]
    fn git_sync_config_serialization_does_not_include_credentials() {
        let settings = AppSettings::default();
        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains("token"));
        assert!(!json.contains("password"));
    }

    #[test]
    fn old_settings_shape_still_loads_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{"git_sync":{"remote_url":"git@example.com:repo.git"}}"#,
        )
        .unwrap();

        let bytes = fs::read(&path).unwrap();
        let settings: AppSettings = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(
            settings.general.startup_panel,
            super::StartupPanelPreference::Dashboard
        );
        assert_eq!(settings.ai.protocol, AiProviderProtocol::OpenAiCompatible);
        assert!(settings.ai.model.is_empty());
        assert!(settings.reminders.notifications_enabled);
        assert_eq!(
            settings.data.default_import_mode,
            ImportModePreference::ReplaceWithBackup
        );
        assert_eq!(settings.git_sync.remote_url, "git@example.com:repo.git");
    }
}
