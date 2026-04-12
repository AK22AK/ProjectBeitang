use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const MAX_SNAPSHOT_BYTES: usize = 95 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct GitRemoteSyncConfig {
    pub remote_url: String,
    pub branch: String,
    pub base_path: String,
    pub enabled: bool,
    pub last_seen_remote_commit: Option<String>,
    pub last_sync_at: Option<DateTime<Utc>>,
}

impl Default for GitRemoteSyncConfig {
    fn default() -> Self {
        Self {
            remote_url: String::new(),
            branch: "main".to_string(),
            base_path: "robinne-sync".to_string(),
            enabled: true,
            last_seen_remote_commit: None,
            last_sync_at: None,
        }
    }
}

impl GitRemoteSyncConfig {
    pub fn normalized(&self) -> Self {
        let mut normalized = self.clone();
        normalized.remote_url = normalized.remote_url.trim().to_string();
        normalized.branch = if normalized.branch.trim().is_empty() {
            "main".to_string()
        } else {
            normalized.branch.trim().to_string()
        };
        normalized.base_path = normalized.base_path.trim_matches('/').trim().to_string();
        normalized
    }

    pub fn validate(&self) -> Result<(), String> {
        let normalized = self.normalized();
        if normalized.remote_url.is_empty() {
            return Err("请填写 Git 远端仓库地址".to_string());
        }
        if normalized.branch.is_empty() {
            return Err("请填写同步分支".to_string());
        }
        Ok(())
    }

    pub fn snapshot_rel_path(&self) -> PathBuf {
        build_rel_path(&self.base_path, "latest.zip")
    }

    pub fn metadata_rel_path(&self) -> PathBuf {
        build_rel_path(&self.base_path, "latest.metadata.json")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitRemoteSyncMetadata {
    pub schema_version: i64,
    pub app_version: String,
    pub exported_at: DateTime<Utc>,
    pub record_count: usize,
    pub attachment_count: usize,
    pub device_label: String,
    pub snapshot_sha256: String,
}

#[derive(Debug, Clone)]
pub struct GitRemoteSyncState {
    pub config: GitRemoteSyncConfig,
}

#[derive(Debug, Clone)]
pub struct GitRemoteVerification {
    pub remote_url: String,
    pub branch: String,
    pub git_version: String,
    pub remote_head_commit: Option<String>,
    pub remote_metadata: Option<GitRemoteSyncMetadata>,
}

#[derive(Debug, Clone)]
pub struct GitRemoteSyncPushResult {
    pub remote_commit: String,
    pub metadata: GitRemoteSyncMetadata,
}

#[derive(Debug, Clone)]
pub struct GitRemoteSyncPullResult {
    pub archive_bytes: Vec<u8>,
    pub remote_commit: String,
    pub metadata: GitRemoteSyncMetadata,
}

#[derive(Debug, Clone)]
pub struct UploadPayload {
    pub snapshot_bytes: Vec<u8>,
    pub metadata: GitRemoteSyncMetadata,
}

#[derive(Debug, Clone)]
pub struct ExportSummary {
    pub schema_version: i64,
    pub record_count: usize,
    pub attachment_count: usize,
}

pub struct GitRemoteSyncClient;

impl GitRemoteSyncClient {
    pub fn new() -> Result<Self, String> {
        ensure_git_available()?;
        Ok(Self)
    }

    pub fn verify_remote(
        &self,
        config: &GitRemoteSyncConfig,
    ) -> Result<GitRemoteVerification, String> {
        let config = config.normalized();
        config.validate()?;
        let git_version = git_version()?;
        let remote_head_commit = ls_remote_head(&config)?;
        let remote_metadata = if remote_head_commit.is_some() {
            match read_remote_metadata(&config) {
                Ok(metadata) => Some(metadata),
                Err(err) if err.contains("缺少") => None,
                Err(err) => return Err(err),
            }
        } else {
            None
        };

        Ok(GitRemoteVerification {
            remote_url: config.remote_url,
            branch: config.branch,
            git_version,
            remote_head_commit,
            remote_metadata,
        })
    }

    pub fn push_snapshot(
        &self,
        config: &GitRemoteSyncConfig,
        payload: UploadPayload,
    ) -> Result<GitRemoteSyncPushResult, String> {
        let config = config.normalized();
        config.validate()?;
        ensure_snapshot_size(payload.snapshot_bytes.len())?;

        let remote_head_commit = ls_remote_head(&config)?;
        ensure_remote_commit_matches(
            config.last_seen_remote_commit.as_deref(),
            remote_head_commit.as_deref(),
        )?;

        let checkout = prepare_checkout(&config, remote_head_commit.as_deref())?;
        write_sync_files(
            &checkout.root,
            &config,
            &payload.snapshot_bytes,
            &payload.metadata,
        )?;
        git_add_paths(
            &checkout.root,
            &[config.snapshot_rel_path(), config.metadata_rel_path()],
        )?;

        if !has_staged_changes(&checkout.root)? {
            let commit = remote_head_commit
                .ok_or_else(|| "远端仓库为空，但本地没有可提交的同步内容".to_string())?;
            return Ok(GitRemoteSyncPushResult {
                remote_commit: commit,
                metadata: payload.metadata,
            });
        }

        git_commit(&checkout.root, "chore: update robinne sync snapshot")?;
        let local_commit = git_rev_parse(&checkout.root, "HEAD")?;
        git_push_branch(&checkout.root, &config.branch)?;

        Ok(GitRemoteSyncPushResult {
            remote_commit: local_commit,
            metadata: payload.metadata,
        })
    }

    pub fn pull_snapshot(
        &self,
        config: &GitRemoteSyncConfig,
    ) -> Result<GitRemoteSyncPullResult, String> {
        let config = config.normalized();
        config.validate()?;

        let remote_commit = ls_remote_head(&config)?
            .ok_or_else(|| "远端分支还没有同步快照，请先执行推送".to_string())?;
        let checkout = prepare_checkout(&config, Some(&remote_commit))?;
        let metadata = read_metadata_from_checkout(&checkout.root, &config)?;
        let snapshot_path = checkout.root.join(config.snapshot_rel_path());
        let archive_bytes = fs::read(&snapshot_path)
            .map_err(|err| format!("读取远端同步包失败 {}: {}", snapshot_path.display(), err))?;
        ensure_snapshot_size(archive_bytes.len())?;

        Ok(GitRemoteSyncPullResult {
            archive_bytes,
            remote_commit,
            metadata,
        })
    }
}

pub fn compute_sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<String>()
}

pub fn build_metadata(export: &ExportSummary, snapshot_bytes: &[u8]) -> GitRemoteSyncMetadata {
    GitRemoteSyncMetadata {
        schema_version: export.schema_version,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        exported_at: Utc::now(),
        record_count: export.record_count,
        attachment_count: export.attachment_count,
        device_label: whoami::fallible::hostname().unwrap_or_else(|_| "unknown-device".to_string()),
        snapshot_sha256: compute_sha256_hex(snapshot_bytes),
    }
}

pub fn build_upload_payload(
    export: &ExportSummary,
    snapshot_bytes: Vec<u8>,
) -> Result<UploadPayload, String> {
    ensure_snapshot_size(snapshot_bytes.len())?;
    let metadata = build_metadata(export, &snapshot_bytes);
    Ok(UploadPayload {
        snapshot_bytes,
        metadata,
    })
}

pub fn ensure_snapshot_size(size: usize) -> Result<(), String> {
    if size > MAX_SNAPSHOT_BYTES {
        return Err("同步包超过 95 MB，请先清理附件后再同步".to_string());
    }
    Ok(())
}

pub fn ensure_remote_commit_matches(
    last_seen_remote_commit: Option<&str>,
    remote_head_commit: Option<&str>,
) -> Result<(), String> {
    match (last_seen_remote_commit, remote_head_commit) {
        (Some(expected), Some(remote)) if expected != remote => {
            Err("远端已有新快照，请先拉取".to_string())
        }
        (Some(_), None) => Err("远端分支状态已变化，请先拉取确认".to_string()),
        _ => Ok(()),
    }
}

fn build_rel_path(base_path: &str, file_name: &str) -> PathBuf {
    let base_path = base_path.trim_matches('/').trim();
    if base_path.is_empty() {
        PathBuf::from(file_name)
    } else {
        PathBuf::from(base_path).join(file_name)
    }
}

fn git_version() -> Result<String, String> {
    let output = run_git(None, ["--version"])?;
    Ok(output.trim().to_string())
}

fn ensure_git_available() -> Result<(), String> {
    git_version().map(|_| ())
}

fn ls_remote_head(config: &GitRemoteSyncConfig) -> Result<Option<String>, String> {
    let ref_name = format!("refs/heads/{}", config.branch);
    let output = run_git(
        None,
        [
            "ls-remote",
            "--heads",
            config.remote_url.as_str(),
            ref_name.as_str(),
        ],
    )?;
    let head = output
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .map(|value| value.to_string());
    Ok(head)
}

fn read_remote_metadata(config: &GitRemoteSyncConfig) -> Result<GitRemoteSyncMetadata, String> {
    let remote_commit = ls_remote_head(config)?.ok_or_else(|| "远端分支还不存在".to_string())?;
    let checkout = prepare_checkout(config, Some(&remote_commit))?;
    read_metadata_from_checkout(&checkout.root, config)
}

fn read_metadata_from_checkout(
    root: &Path,
    config: &GitRemoteSyncConfig,
) -> Result<GitRemoteSyncMetadata, String> {
    let metadata_path = root.join(config.metadata_rel_path());
    let bytes = fs::read(&metadata_path).map_err(|err| {
        format!(
            "远端同步元信息缺少或无法读取 {}: {}",
            metadata_path.display(),
            err
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|err| {
        format!(
            "解析远端同步元信息失败 {}: {}",
            metadata_path.display(),
            err
        )
    })
}

struct CheckoutRepo {
    _temp_dir: TempDir,
    root: PathBuf,
}

fn prepare_checkout(
    config: &GitRemoteSyncConfig,
    remote_head_commit: Option<&str>,
) -> Result<CheckoutRepo, String> {
    let temp_dir = tempfile::tempdir().map_err(|err| format!("创建临时 Git 目录失败: {}", err))?;
    let root = temp_dir.path().to_path_buf();

    if remote_head_commit.is_some() {
        run_git(
            None,
            [
                "clone",
                "--depth",
                "1",
                "--branch",
                config.branch.as_str(),
                config.remote_url.as_str(),
                root.to_string_lossy().as_ref(),
            ],
        )?;
    } else {
        run_git(Some(&root), ["init"])?;
        git_set_user_identity(&root)?;
        run_git(
            Some(&root),
            ["remote", "add", "origin", config.remote_url.as_str()],
        )?;
        run_git(
            Some(&root),
            ["checkout", "--orphan", config.branch.as_str()],
        )?;
    }

    git_set_user_identity(&root)?;

    Ok(CheckoutRepo {
        _temp_dir: temp_dir,
        root,
    })
}

fn write_sync_files(
    root: &Path,
    config: &GitRemoteSyncConfig,
    snapshot_bytes: &[u8],
    metadata: &GitRemoteSyncMetadata,
) -> Result<(), String> {
    let snapshot_path = root.join(config.snapshot_rel_path());
    let metadata_path = root.join(config.metadata_rel_path());

    if let Some(parent) = snapshot_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("创建同步目录失败 {}: {}", parent.display(), err))?;
    }
    if let Some(parent) = metadata_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("创建同步目录失败 {}: {}", parent.display(), err))?;
    }

    fs::write(&snapshot_path, snapshot_bytes)
        .map_err(|err| format!("写入同步快照失败 {}: {}", snapshot_path.display(), err))?;
    let metadata_bytes = serde_json::to_vec_pretty(metadata)
        .map_err(|err| format!("序列化同步元信息失败: {}", err))?;
    fs::write(&metadata_path, metadata_bytes)
        .map_err(|err| format!("写入同步元信息失败 {}: {}", metadata_path.display(), err))?;

    Ok(())
}

fn git_add_paths(root: &Path, paths: &[PathBuf]) -> Result<(), String> {
    let mut command = Command::new("git");
    command.current_dir(root).arg("add");
    for path in paths {
        command.arg(path.as_os_str());
    }
    run_git_command(command, "执行 git add 失败").map(|_| ())
}

fn has_staged_changes(root: &Path) -> Result<bool, String> {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args(["diff", "--cached", "--quiet", "--exit-code"]);
    match command.output() {
        Ok(output) => Ok(!output.status.success()),
        Err(err) => Err(format!("检查 Git 暂存区失败: {}", err)),
    }
}

fn git_commit(root: &Path, message: &str) -> Result<(), String> {
    run_git(Some(root), ["commit", "-m", message]).map(|_| ())
}

fn git_push_branch(root: &Path, branch: &str) -> Result<(), String> {
    run_git(
        Some(root),
        ["push", "origin", &format!("HEAD:refs/heads/{branch}")],
    )
    .map(|_| ())
}

fn git_rev_parse(root: &Path, rev: &str) -> Result<String, String> {
    let output = run_git(Some(root), ["rev-parse", rev])?;
    Ok(output.trim().to_string())
}

fn git_set_user_identity(root: &Path) -> Result<(), String> {
    run_git(Some(root), ["config", "user.name", "Robinne Sync"])?;
    run_git(
        Some(root),
        ["config", "user.email", "robinne-sync@local.invalid"],
    )?;
    Ok(())
}

fn run_git<const N: usize>(cwd: Option<&Path>, args: [&str; N]) -> Result<String, String> {
    let mut command = Command::new("git");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.args(args);
    run_git_command(command, "执行 git 命令失败")
}

fn run_git_command(mut command: Command, context: &str) -> Result<String, String> {
    command.env("GIT_TERMINAL_PROMPT", "0");
    let printable = format_command(&command);
    let output = command
        .output()
        .map_err(|err| format!("{context}: {} ({printable})", err))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        Err(format!("{context}: {} ({printable})", detail))
    }
}

fn format_command(command: &Command) -> String {
    let program = command.get_program().to_string_lossy();
    let args = command
        .get_args()
        .map(os_to_string)
        .collect::<Vec<_>>()
        .join(" ");
    format!("{program} {args}").trim().to_string()
}

fn os_to_string(value: &OsStr) -> String {
    value.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::{build_metadata, compute_sha256_hex, ensure_remote_commit_matches, ExportSummary};

    #[test]
    fn metadata_contains_snapshot_hash() {
        let export = ExportSummary {
            schema_version: 7,
            record_count: 2,
            attachment_count: 1,
        };
        let bytes = b"robinne-sync".to_vec();
        let metadata = build_metadata(&export, &bytes);
        assert_eq!(metadata.schema_version, 7);
        assert_eq!(metadata.snapshot_sha256, compute_sha256_hex(&bytes));
    }

    #[test]
    fn remote_commit_mismatch_is_rejected() {
        let error = ensure_remote_commit_matches(Some("local"), Some("remote")).unwrap_err();
        assert!(error.contains("请先拉取"));
    }
}
