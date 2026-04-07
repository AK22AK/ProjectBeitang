use crate::attachment_image::read_source_image_metadata;
use gpui::{px, Image, ImageFormat, Pixels};
use image::ImageFormat as SourceImageFormat;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct PendingAttachment {
    pub path: PathBuf,
    pub file_name: String,
    pub file_size: usize,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    pub preview_image: Option<Arc<Image>>,
}

pub fn prepare_pending_attachments(paths: Vec<PathBuf>) -> Result<Vec<PendingAttachment>, String> {
    paths
        .into_iter()
        .map(prepare_single_pending_attachment)
        .collect()
}

pub fn format_attachment_meta(attachment: &PendingAttachment) -> String {
    format!(
        "{} × {} · {}",
        attachment.width,
        attachment.height,
        format_file_size(attachment.file_size)
    )
}

pub fn attachment_preview_size(attachment: &PendingAttachment) -> (Pixels, Pixels) {
    const MAX_WIDTH: f32 = 180.0;
    const MAX_HEIGHT: f32 = 120.0;

    let width = attachment.width.max(1) as f32;
    let height = attachment.height.max(1) as f32;
    let scale = (MAX_WIDTH / width).min(MAX_HEIGHT / height).min(1.0);

    (px(width * scale), px(height * scale))
}

fn preview_image_from_bytes(mime_type: &str, bytes: Vec<u8>) -> Option<Arc<Image>> {
    let format = ImageFormat::from_mime_type(mime_type)?;
    Some(Arc::new(Image::from_bytes(format, bytes)))
}

fn prepare_single_pending_attachment(path: PathBuf) -> Result<PendingAttachment, String> {
    let source_bytes =
        fs::read(&path).map_err(|err| format!("读取图片失败 {}: {}", path.display(), err))?;
    let metadata = read_source_image_metadata(&path)?;
    let format = image_format_from_mime_type(&metadata.mime_type)
        .ok_or_else(|| format!("暂不支持的图片格式: {}", path.display()))?;
    ensure_supported_format(format, &path)?;

    Ok(PendingAttachment {
        path,
        file_name: metadata.file_name,
        file_size: metadata.file_size,
        mime_type: metadata.mime_type.clone(),
        width: metadata.width,
        height: metadata.height,
        preview_image: preview_image_from_bytes(&metadata.mime_type, source_bytes),
    })
}

fn ensure_supported_format(format: SourceImageFormat, path: &PathBuf) -> Result<(), String> {
    match format {
        SourceImageFormat::Png
        | SourceImageFormat::Jpeg
        | SourceImageFormat::Gif
        | SourceImageFormat::WebP => Ok(()),
        _ => Err(format!("暂不支持的图片格式: {}", path.display())),
    }
}

fn image_format_from_mime_type(mime_type: &str) -> Option<SourceImageFormat> {
    match mime_type {
        "image/png" => Some(SourceImageFormat::Png),
        "image/jpeg" => Some(SourceImageFormat::Jpeg),
        "image/gif" => Some(SourceImageFormat::Gif),
        "image/webp" => Some(SourceImageFormat::WebP),
        _ => None,
    }
}

fn format_file_size(file_size: usize) -> String {
    if file_size >= 1024 * 1024 {
        format!("{:.1} MB", file_size as f64 / (1024.0 * 1024.0))
    } else if file_size >= 1024 {
        format!("{:.1} KB", file_size as f64 / 1024.0)
    } else {
        format!("{} B", file_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};
    use tempfile::TempDir;

    #[test]
    fn test_prepare_pending_attachments_keeps_source_path_and_preview() {
        let temp_dir = TempDir::new().unwrap();
        let image_path = temp_dir.path().join("draft.png");
        let image: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(120, 80, Rgb([255, 120, 0]));
        image.save(&image_path).unwrap();

        let attachments = prepare_pending_attachments(vec![image_path.clone()]).unwrap();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].path, image_path);
        assert_eq!(attachments[0].width, 120);
        assert_eq!(attachments[0].height, 80);
        assert!(attachments[0].preview_image.is_some());
    }

    #[test]
    fn test_attachment_preview_size_stays_within_bounds() {
        let attachment = PendingAttachment {
            path: PathBuf::from("/tmp/large.png"),
            file_name: "large.png".to_string(),
            file_size: 1024,
            mime_type: "image/png".to_string(),
            width: 2048,
            height: 512,
            preview_image: None,
        };

        let (width, height) = attachment_preview_size(&attachment);
        assert!(width <= px(180.0));
        assert!(height <= px(120.0));
    }
}
