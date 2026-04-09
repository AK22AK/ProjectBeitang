use crate::models::Attachment;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    unsafe extern "C" {
        fn bt_quicklook_preview_file(
            path: *const c_char,
            error_buffer: *mut c_char,
            error_buffer_len: usize,
        ) -> bool;
    }

    fn preview_cache_dir() -> Result<PathBuf, String> {
        let base = dirs::cache_dir()
            .or_else(dirs::data_local_dir)
            .or_else(dirs::data_dir)
            .ok_or_else(|| "无法确定预览缓存目录".to_string())?;
        let dir = base.join("robinne").join("quicklook");
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

    fn ffi_open_path(path: &Path) -> Result<(), String> {
        let path_str = path
            .to_str()
            .ok_or_else(|| format!("预览路径不是有效 UTF-8: {}", path.display()))?;
        let c_path = CString::new(path_str)
            .map_err(|_| format!("预览路径包含非法字符: {}", path.display()))?;
        let mut error_buffer = vec![0 as c_char; 1024];
        let success = unsafe {
            bt_quicklook_preview_file(
                c_path.as_ptr(),
                error_buffer.as_mut_ptr(),
                error_buffer.len(),
            )
        };
        if success {
            return Ok(());
        }

        let message = unsafe { CStr::from_ptr(error_buffer.as_ptr()) }
            .to_string_lossy()
            .trim()
            .to_string();
        if message.is_empty() {
            Err(format!("打开系统预览失败: {}", path.display()))
        } else {
            Err(message)
        }
    }

    pub fn open_path(path: &Path) -> Result<(), String> {
        let canonical_path = path
            .canonicalize()
            .map_err(|err| format!("预览文件不可访问 {}: {}", path.display(), err))?;
        ffi_open_path(&canonical_path)
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
