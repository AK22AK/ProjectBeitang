use crate::models::Attachment;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    fn preview_cache_dir() -> Result<PathBuf, String> {
        let base = dirs::cache_dir()
            .or_else(dirs::data_local_dir)
            .or_else(dirs::data_dir)
            .ok_or_else(|| "无法确定预览缓存目录".to_string())?;
        let dir = base.join("beitang").join("quicklook");
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("创建预览缓存目录失败 {}: {}", dir.display(), err))?;
        Ok(dir)
    }

    fn attachment_extension(attachment: &Attachment) -> String {
        Path::new(&attachment.file_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .filter(|ext| !ext.is_empty())
            .map(|ext| ext.to_string())
            .unwrap_or_else(|| match attachment.mime_type.as_str() {
                "image/png" => "png".to_string(),
                "image/jpeg" => "jpg".to_string(),
                "image/gif" => "gif".to_string(),
                "image/webp" => "webp".to_string(),
                "image/heic" => "heic".to_string(),
                "image/heif" => "heif".to_string(),
                _ => "bin".to_string(),
            })
    }

    pub fn export_attachment_to_cache(
        attachment: &Attachment,
        bytes: &[u8],
    ) -> Result<PathBuf, String> {
        let dir = preview_cache_dir()?;
        let path = dir.join(format!(
            "{}.{}",
            attachment.id,
            attachment_extension(attachment)
        ));
        std::fs::write(&path, bytes)
            .map_err(|err| format!("写入预览缓存失败 {}: {}", path.display(), err))?;
        Ok(path)
    }

    pub fn open_path(path: &Path) -> Result<(), String> {
        let canonical_path = path
            .canonicalize()
            .map_err(|err| format!("预览文件不可访问 {}: {}", path.display(), err))?;
        Command::new("qlmanage")
            .arg("-p")
            .arg(&canonical_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("启动系统预览失败 {}: {}", canonical_path.display(), err))
    }
}

#[cfg(target_os = "macos")]
pub fn open_path(path: &Path) -> Result<(), String> {
    macos::open_path(path)
}

#[cfg(target_os = "macos")]
pub fn open_saved_attachment(
    attachment: &Attachment,
    file_data: Option<Vec<u8>>,
) -> Result<(), String> {
    if let Some(bytes) = file_data {
        let path = macos::export_attachment_to_cache(attachment, &bytes)?;
        return macos::open_path(&path);
    }

    let file_path = Path::new(&attachment.file_path);
    if file_path.exists() {
        return macos::open_path(file_path);
    }

    if let Some(source_path) = attachment.source_path.as_deref() {
        let source_path = Path::new(source_path);
        if source_path.exists() {
            return macos::open_path(source_path);
        }
    }

    Err(format!(
        "预览图片失败：{} 没有可用文件内容",
        attachment.file_name
    ))
}

#[cfg(not(target_os = "macos"))]
pub fn open_path(_path: &Path) -> Result<(), String> {
    Err("当前平台暂不支持系统图片预览".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn open_saved_attachment(
    _attachment: &Attachment,
    _file_data: Option<Vec<u8>>,
) -> Result<(), String> {
    Err("当前平台暂不支持系统图片预览".to_string())
}
