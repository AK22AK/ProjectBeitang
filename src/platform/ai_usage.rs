use crate::ai::{AiProviderProtocol, AiUsage};
use crate::data_management::app_data_dir;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const AI_USAGE_FILE_NAME: &str = "ai_usage.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AiUsageEventKind {
    TestConnection,
    GenerateSummary,
}

impl AiUsageEventKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::TestConnection => "测试连接",
            Self::GenerateSummary => "生成总结",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiDailyUsageEntry {
    pub date: String,
    pub protocol: AiProviderProtocol,
    pub input_tokens_total: u64,
    pub output_tokens_total: u64,
    pub request_count: u64,
}

impl AiDailyUsageEntry {
    pub fn usage(&self) -> AiUsage {
        AiUsage {
            input_tokens: self.input_tokens_total,
            output_tokens: self.output_tokens_total,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiLatestUsageSnapshot {
    pub date: String,
    pub protocol: AiProviderProtocol,
    pub event_kind: AiUsageEventKind,
    pub usage: Option<AiUsage>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AiUsageFile {
    #[serde(default)]
    daily_entries: Vec<AiDailyUsageEntry>,
    #[serde(default)]
    latest_snapshots: Vec<AiLatestUsageSnapshot>,
}

pub fn ai_usage_file_path() -> PathBuf {
    app_data_dir().join(AI_USAGE_FILE_NAME)
}

fn today_key() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

fn load_usage_file(path: &Path) -> Result<AiUsageFile, String> {
    if !path.exists() {
        return Ok(AiUsageFile::default());
    }

    let bytes = fs::read(path)
        .map_err(|err| format!("读取 AI usage 文件失败 {}: {}", path.display(), err))?;
    serde_json::from_slice(&bytes)
        .map_err(|err| format!("解析 AI usage 文件失败 {}: {}", path.display(), err))
}

fn save_usage_file(path: &Path, usage_file: &AiUsageFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("创建 AI usage 目录失败 {}: {}", parent.display(), err))?;
    }

    let bytes = serde_json::to_vec_pretty(usage_file)
        .map_err(|err| format!("序列化 AI usage 文件失败 {}: {}", path.display(), err))?;
    fs::write(path, bytes)
        .map_err(|err| format!("写入 AI usage 文件失败 {}: {}", path.display(), err))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = fs::Permissions::from_mode(0o600);
        fs::set_permissions(path, permissions)
            .map_err(|err| format!("设置 AI usage 文件权限失败 {}: {}", path.display(), err))?;
    }

    Ok(())
}

fn record_ai_usage_at(
    path: &Path,
    protocol: AiProviderProtocol,
    event_kind: AiUsageEventKind,
    usage: Option<AiUsage>,
    date: &str,
) -> Result<(), String> {
    let mut usage_file = load_usage_file(path)?;

    usage_file
        .latest_snapshots
        .retain(|entry| !(entry.protocol == protocol && entry.date != date));

    if let Some(snapshot) = usage_file
        .latest_snapshots
        .iter_mut()
        .find(|entry| entry.protocol == protocol)
    {
        snapshot.date = date.to_string();
        snapshot.event_kind = event_kind;
        snapshot.usage = usage;
    } else {
        usage_file.latest_snapshots.push(AiLatestUsageSnapshot {
            date: date.to_string(),
            protocol,
            event_kind,
            usage,
        });
    }

    if let Some(usage) = usage {
        if let Some(entry) = usage_file
            .daily_entries
            .iter_mut()
            .find(|entry| entry.protocol == protocol && entry.date == date)
        {
            entry.input_tokens_total = entry.input_tokens_total.saturating_add(usage.input_tokens);
            entry.output_tokens_total = entry
                .output_tokens_total
                .saturating_add(usage.output_tokens);
            entry.request_count = entry.request_count.saturating_add(1);
        } else {
            usage_file.daily_entries.push(AiDailyUsageEntry {
                date: date.to_string(),
                protocol,
                input_tokens_total: usage.input_tokens,
                output_tokens_total: usage.output_tokens,
                request_count: 1,
            });
        }
    }

    save_usage_file(path, &usage_file)
}

pub fn record_ai_usage(
    protocol: AiProviderProtocol,
    event_kind: AiUsageEventKind,
    usage: Option<AiUsage>,
) -> Result<(), String> {
    let path = ai_usage_file_path();
    record_ai_usage_at(&path, protocol, event_kind, usage, &today_key())
}

pub fn load_today_ai_usage(
    protocol: AiProviderProtocol,
) -> Result<Option<AiDailyUsageEntry>, String> {
    let path = ai_usage_file_path();
    let usage_file = load_usage_file(&path)?;
    let today = today_key();
    Ok(usage_file
        .daily_entries
        .into_iter()
        .find(|entry| entry.protocol == protocol && entry.date == today))
}

pub fn load_latest_ai_usage(
    protocol: AiProviderProtocol,
) -> Result<Option<AiLatestUsageSnapshot>, String> {
    let path = ai_usage_file_path();
    let usage_file = load_usage_file(&path)?;
    let today = today_key();
    Ok(usage_file
        .latest_snapshots
        .into_iter()
        .find(|entry| entry.protocol == protocol && entry.date == today))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn recording_usage_accumulates_daily_totals_and_updates_latest_snapshot() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ai_usage.json");
        let date = "2026-04-14";

        record_ai_usage_at(
            &path,
            AiProviderProtocol::OpenAiCompatible,
            AiUsageEventKind::GenerateSummary,
            Some(AiUsage {
                input_tokens: 10,
                output_tokens: 5,
            }),
            date,
        )
        .unwrap();

        record_ai_usage_at(
            &path,
            AiProviderProtocol::OpenAiCompatible,
            AiUsageEventKind::TestConnection,
            Some(AiUsage {
                input_tokens: 2,
                output_tokens: 1,
            }),
            date,
        )
        .unwrap();

        let usage_file = load_usage_file(&path).unwrap();
        let daily = usage_file
            .daily_entries
            .iter()
            .find(|entry| entry.protocol == AiProviderProtocol::OpenAiCompatible)
            .unwrap();
        assert_eq!(daily.input_tokens_total, 12);
        assert_eq!(daily.output_tokens_total, 6);
        assert_eq!(daily.request_count, 2);

        let latest = usage_file
            .latest_snapshots
            .iter()
            .find(|entry| entry.protocol == AiProviderProtocol::OpenAiCompatible)
            .unwrap();
        assert_eq!(latest.event_kind, AiUsageEventKind::TestConnection);
        assert_eq!(
            latest.usage,
            Some(AiUsage {
                input_tokens: 2,
                output_tokens: 1,
            })
        );
    }

    #[test]
    fn latest_snapshot_is_stored_even_when_usage_is_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ai_usage.json");

        record_ai_usage_at(
            &path,
            AiProviderProtocol::Anthropic,
            AiUsageEventKind::GenerateSummary,
            None,
            "2026-04-14",
        )
        .unwrap();

        let usage_file = load_usage_file(&path).unwrap();
        assert!(usage_file.daily_entries.is_empty());
        assert_eq!(usage_file.latest_snapshots.len(), 1);
        assert_eq!(usage_file.latest_snapshots[0].usage, None);
    }
}
