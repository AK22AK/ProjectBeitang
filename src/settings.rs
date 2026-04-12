use crate::data_management::app_data_dir;
use crate::git_sync::GitRemoteSyncConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const SETTINGS_FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    #[serde(default)]
    pub git_sync: GitRemoteSyncConfig,
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
    use super::AppSettings;

    #[test]
    fn git_sync_config_serialization_does_not_include_credentials() {
        let settings = AppSettings::default();
        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains("token"));
        assert!(!json.contains("password"));
    }
}
