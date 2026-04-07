use crate::models::{Attachment, AttachmentStatus};
use chrono::Utc;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::FilterType;
use image::{ColorType, DynamicImage, GenericImageView, ImageEncoder, ImageFormat, ImageReader};
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use uuid::Uuid;

const MAX_IMAGE_DIMENSION: u32 = 2048;
const JPEG_QUALITY: u8 = 82;

pub struct PreparedImageAttachment {
    pub attachment: Attachment,
    pub file_data: Vec<u8>,
}

pub struct SourceImageMetadata {
    pub file_name: String,
    pub file_size: usize,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
}

pub struct AttachmentImportJob {
    pub attachment: Attachment,
    pub path: PathBuf,
}

pub fn prepare_image_attachments(
    record_id: Uuid,
    paths: Vec<PathBuf>,
) -> Result<Vec<PreparedImageAttachment>, String> {
    paths
        .into_iter()
        .map(|path| prepare_single_image_attachment(record_id.to_string(), path, None))
        .collect()
}

pub fn build_attachment_import_jobs(
    record_id: Uuid,
    paths: Vec<PathBuf>,
) -> Result<Vec<AttachmentImportJob>, String> {
    paths
        .into_iter()
        .map(|path| build_single_attachment_import_job(record_id.to_string(), path))
        .collect()
}

pub fn prepare_attachment_for_existing_id(
    record_id: &str,
    attachment_id: &str,
    path: PathBuf,
) -> Result<PreparedImageAttachment, String> {
    prepare_single_image_attachment(record_id.to_string(), path, Some(attachment_id.to_string()))
}

pub fn read_source_image_metadata(path: &std::path::Path) -> Result<SourceImageMetadata, String> {
    let source_bytes =
        fs::read(path).map_err(|err| format!("读取图片失败 {}: {}", path.display(), err))?;
    let mut reader = ImageReader::new(Cursor::new(&source_bytes));
    reader = reader
        .with_guessed_format()
        .map_err(|err| format!("识别图片格式失败 {}: {}", path.display(), err))?;

    let Some(format) = reader.format() else {
        return Err(format!("无法识别图片格式: {}", path.display()));
    };

    ensure_supported_format(format, &path.to_path_buf())?;

    let (width, height) = reader
        .into_dimensions()
        .map_err(|err| format!("读取图片尺寸失败 {}: {}", path.display(), err))?;

    Ok(SourceImageMetadata {
        file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
            .unwrap_or_else(|| "image".to_string()),
        file_size: source_bytes.len(),
        mime_type: format_to_mime_type(format).to_string(),
        width,
        height,
    })
}

fn build_single_attachment_import_job(
    record_id: String,
    path: PathBuf,
) -> Result<AttachmentImportJob, String> {
    let metadata = read_source_image_metadata(&path)?;
    let attachment_id = Uuid::new_v4().to_string();

    Ok(AttachmentImportJob {
        attachment: Attachment {
            id: attachment_id.clone(),
            record_id,
            file_name: metadata.file_name,
            file_path: format!("db://attachment/{}", attachment_id),
            file_size: metadata.file_size,
            mime_type: metadata.mime_type,
            width: metadata.width,
            height: metadata.height,
            created_at: Utc::now(),
            status: AttachmentStatus::Processing,
            error_message: None,
            source_path: Some(path.to_string_lossy().to_string()),
        },
        path,
    })
}

fn prepare_single_image_attachment(
    record_id: String,
    path: PathBuf,
    attachment_id: Option<String>,
) -> Result<PreparedImageAttachment, String> {
    let source_bytes =
        fs::read(&path).map_err(|err| format!("读取图片失败 {}: {}", path.display(), err))?;
    let mut reader = ImageReader::new(Cursor::new(source_bytes));
    reader = reader
        .with_guessed_format()
        .map_err(|err| format!("识别图片格式失败 {}: {}", path.display(), err))?;

    let Some(format) = reader.format() else {
        return Err(format!("无法识别图片格式: {}", path.display()));
    };

    ensure_supported_format(format, &path)?;

    let decoded = reader
        .decode()
        .map_err(|err| format!("解码图片失败 {}: {}", path.display(), err))?;
    let resized = resize_if_needed(decoded);
    let has_alpha = resized.color().has_alpha();
    let (file_data, mime_type) = encode_image(&resized, has_alpha)
        .map_err(|err| format!("压缩图片失败 {}: {}", path.display(), err))?;
    let (width, height) = resized.dimensions();
    let attachment_id = attachment_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .unwrap_or_else(|| "image".to_string());

    Ok(PreparedImageAttachment {
        attachment: Attachment {
            id: attachment_id.clone(),
            record_id,
            file_name,
            file_path: format!("db://attachment/{}", attachment_id),
            file_size: file_data.len(),
            mime_type,
            width,
            height,
            created_at: Utc::now(),
            status: AttachmentStatus::Ready,
            error_message: None,
            source_path: None,
        },
        file_data,
    })
}

fn ensure_supported_format(format: ImageFormat, path: &PathBuf) -> Result<(), String> {
    match format {
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif | ImageFormat::WebP => Ok(()),
        _ => Err(format!("暂不支持的图片格式: {}", path.display())),
    }
}

fn format_to_mime_type(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Gif => "image/gif",
        ImageFormat::WebP => "image/webp",
        _ => "application/octet-stream",
    }
}

fn resize_if_needed(image: DynamicImage) -> DynamicImage {
    let (width, height) = image.dimensions();
    let longest = width.max(height);
    if longest <= MAX_IMAGE_DIMENSION {
        return image;
    }

    let scale = MAX_IMAGE_DIMENSION as f32 / longest as f32;
    let target_width = ((width as f32) * scale).round().max(1.0) as u32;
    let target_height = ((height as f32) * scale).round().max(1.0) as u32;
    image.resize(target_width, target_height, FilterType::Lanczos3)
}

fn encode_image(
    image: &DynamicImage,
    has_alpha: bool,
) -> Result<(Vec<u8>, String), image::ImageError> {
    if has_alpha {
        let rgba = image.to_rgba8();
        let mut bytes = Vec::new();
        PngEncoder::new(&mut bytes).write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            ColorType::Rgba8.into(),
        )?;
        Ok((bytes, "image/png".to_string()))
    } else {
        let rgb = image.to_rgb8();
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY).write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ColorType::Rgb8.into(),
        )?;
        Ok((bytes, "image/jpeg".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb, Rgba};
    use tempfile::TempDir;

    #[test]
    fn test_prepare_opaque_png_encodes_to_jpeg() {
        let temp_dir = TempDir::new().unwrap();
        let image_path = temp_dir.path().join("opaque.png");
        let image: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(64, 32, Rgb([100, 120, 140]));
        image.save(&image_path).unwrap();

        let prepared = prepare_image_attachments(Uuid::new_v4(), vec![image_path]).unwrap();
        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].attachment.mime_type, "image/jpeg");
        assert_eq!(prepared[0].attachment.width, 64);
        assert_eq!(prepared[0].attachment.height, 32);
        assert!(!prepared[0].file_data.is_empty());
    }

    #[test]
    fn test_prepare_transparent_png_keeps_png() {
        let temp_dir = TempDir::new().unwrap();
        let image_path = temp_dir.path().join("alpha.png");
        let image: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(48, 48, Rgba([255, 0, 0, 128]));
        image.save(&image_path).unwrap();

        let prepared = prepare_image_attachments(Uuid::new_v4(), vec![image_path]).unwrap();
        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].attachment.mime_type, "image/png");
        assert!(!prepared[0].file_data.is_empty());
    }
}
