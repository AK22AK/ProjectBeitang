use crate::ai::AiProviderProtocol;
use crate::data_management::app_data_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const SECRETS_FILE_NAME: &str = "secrets.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretSource {
    LocalFile,
    Environment(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedSecret {
    pub value: String,
    pub source: SecretSource,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SecretFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    openai_compatible_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    anthropic_api_key: Option<String>,
}

impl SecretFile {
    fn get(&self, protocol: AiProviderProtocol) -> Option<&str> {
        match protocol {
            AiProviderProtocol::OpenAiCompatible => self.openai_compatible_api_key.as_deref(),
            AiProviderProtocol::Anthropic => self.anthropic_api_key.as_deref(),
        }
    }

    fn set(&mut self, protocol: AiProviderProtocol, value: Option<String>) {
        match protocol {
            AiProviderProtocol::OpenAiCompatible => self.openai_compatible_api_key = value,
            AiProviderProtocol::Anthropic => self.anthropic_api_key = value,
        }
    }

    fn is_empty(&self) -> bool {
        self.openai_compatible_api_key.is_none() && self.anthropic_api_key.is_none()
    }
}

pub fn secrets_file_path() -> PathBuf {
    app_data_dir().join(SECRETS_FILE_NAME)
}

fn trim_secret(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn load_secret_file(path: &Path) -> Result<SecretFile, String> {
    if !path.exists() {
        return Ok(SecretFile::default());
    }

    let bytes =
        fs::read(path).map_err(|err| format!("读取密钥文件失败 {}: {}", path.display(), err))?;
    serde_json::from_slice(&bytes)
        .map_err(|err| format!("解析密钥文件失败 {}: {}", path.display(), err))
}

fn save_secret_file(path: &Path, secrets: &SecretFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("创建密钥目录失败 {}: {}", parent.display(), err))?;
    }

    let bytes = serde_json::to_vec_pretty(secrets)
        .map_err(|err| format!("序列化密钥文件失败 {}: {}", path.display(), err))?;
    fs::write(path, bytes)
        .map_err(|err| format!("写入密钥文件失败 {}: {}", path.display(), err))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = fs::Permissions::from_mode(0o600);
        fs::set_permissions(path, permissions)
            .map_err(|err| format!("设置密钥文件权限失败 {}: {}", path.display(), err))?;
    }

    Ok(())
}

pub fn load_secret(protocol: AiProviderProtocol) -> Result<Option<LoadedSecret>, String> {
    let path = secrets_file_path();
    let secrets = load_secret_file(&path)?;
    if let Some(secret) = secrets.get(protocol).and_then(trim_secret) {
        return Ok(Some(LoadedSecret {
            value: secret,
            source: SecretSource::LocalFile,
        }));
    }

    if let Some(secret) = std::env::var(protocol.api_key_env_var())
        .ok()
        .as_deref()
        .and_then(trim_secret)
    {
        return Ok(Some(LoadedSecret {
            value: secret,
            source: SecretSource::Environment(protocol.api_key_env_var()),
        }));
    }

    Ok(None)
}

pub fn save_secret(protocol: AiProviderProtocol, secret: &str) -> Result<(), String> {
    let Some(secret) = trim_secret(secret) else {
        return Err("API Key 不能为空".to_string());
    };

    let path = secrets_file_path();
    let mut secrets = load_secret_file(&path)?;
    secrets.set(protocol, Some(secret));
    save_secret_file(&path, &secrets)
}

pub fn delete_secret(protocol: AiProviderProtocol) -> Result<(), String> {
    let path = secrets_file_path();
    let mut secrets = load_secret_file(&path)?;
    secrets.set(protocol, None);

    if secrets.is_empty() {
        if path.exists() {
            fs::remove_file(&path)
                .map_err(|err| format!("删除密钥文件失败 {}: {}", path.display(), err))?;
        }
        return Ok(());
    }

    save_secret_file(&path, &secrets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn secret_file_roundtrip_and_cleanup() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("secrets.json");

        let mut secrets = SecretFile::default();
        secrets.set(
            AiProviderProtocol::OpenAiCompatible,
            Some("test-key".to_string()),
        );
        save_secret_file(&path, &secrets).unwrap();

        let loaded = load_secret_file(&path).unwrap();
        assert_eq!(
            loaded.get(AiProviderProtocol::OpenAiCompatible),
            Some("test-key")
        );

        secrets.set(AiProviderProtocol::OpenAiCompatible, None);
        assert!(secrets.is_empty());
    }

    #[test]
    fn trim_secret_discards_blank_values() {
        assert_eq!(trim_secret("   "), None);
        assert_eq!(trim_secret("  abc  "), Some("abc".to_string()));
    }
}
