use crate::attachment_image::read_source_image_metadata;
use crate::ui::attachment_draft::{
    pending_attachment_from_data, prepare_pending_attachments, PendingAttachment,
};
use gpui::{
    ClipboardEntry, ClipboardItem, Image as ClipboardImage, ImageFormat as ClipboardImageFormat,
};
use image::{DynamicImage, GenericImageView, ImageFormat as SourceImageFormat, ImageReader};
use std::collections::HashSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub fn extract_image_paths_from_clipboard(item: &ClipboardItem) -> Result<Vec<PathBuf>, String> {
    extract_image_paths_in_dir(item, &clipboard_cache_dir()?)
}

pub fn prepare_pending_attachments_from_clipboard(
    item: &ClipboardItem,
) -> Result<Vec<PendingAttachment>, String> {
    prepare_pending_attachments_from_clipboard_in_dir(item, &clipboard_cache_dir()?)
}

pub fn clipboard_has_image_candidate(item: &ClipboardItem) -> bool {
    item.entries().iter().any(|entry| match entry {
        ClipboardEntry::Image(_) => true,
        ClipboardEntry::ExternalPaths(external_paths) => external_paths
            .paths()
            .iter()
            .any(|path| path_looks_like_image(path)),
        ClipboardEntry::String(_) => false,
    })
}

fn extract_image_paths_in_dir(
    item: &ClipboardItem,
    cache_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    let mut seen_paths = HashSet::new();
    let mut image_error = None;
    let mut saw_clipboard_image = false;

    for entry in item.entries() {
        match entry {
            ClipboardEntry::Image(image) => {
                saw_clipboard_image = true;
                match write_clipboard_image_to_dir(image, cache_dir) {
                    Ok(path) => paths.push(path),
                    Err(err) => image_error = Some(err),
                }
            }
            ClipboardEntry::ExternalPaths(external_paths) => {
                for path in external_paths.paths() {
                    if !path.exists() || !path.is_file() {
                        continue;
                    }

                    if read_source_image_metadata(path).is_err() {
                        continue;
                    }

                    let path = path.to_path_buf();
                    if seen_paths.insert(path.clone()) {
                        paths.push(path);
                    }
                }
            }
            ClipboardEntry::String(_) => {}
        }
    }

    if !paths.is_empty() {
        Ok(paths)
    } else if saw_clipboard_image {
        Err(image_error.unwrap_or_else(|| "剪贴板图片暂时无法导入".to_string()))
    } else {
        Ok(Vec::new())
    }
}

fn prepare_pending_attachments_from_clipboard_in_dir(
    item: &ClipboardItem,
    cache_dir: &Path,
) -> Result<Vec<PendingAttachment>, String> {
    let mut attachments = Vec::new();
    let mut image_error = None;
    let mut saw_clipboard_image = false;

    for entry in item.entries() {
        match entry {
            ClipboardEntry::Image(image) => {
                saw_clipboard_image = true;
                match prepare_pending_attachment_from_clipboard_image(image, cache_dir) {
                    Ok(attachment) => attachments.push(attachment),
                    Err(err) => image_error = Some(err),
                }
            }
            ClipboardEntry::ExternalPaths(external_paths) => {
                let paths = collect_external_image_paths(external_paths.paths());
                if !paths.is_empty() {
                    let mut prepared = prepare_pending_attachments(paths)?;
                    attachments.append(&mut prepared);
                }
            }
            ClipboardEntry::String(_) => {}
        }
    }

    if !attachments.is_empty() {
        Ok(attachments)
    } else if saw_clipboard_image {
        Err(image_error.unwrap_or_else(|| "剪贴板图片暂时无法导入".to_string()))
    } else {
        Ok(Vec::new())
    }
}

fn clipboard_cache_dir() -> Result<PathBuf, String> {
    let base = dirs::cache_dir()
        .or_else(dirs::data_local_dir)
        .or_else(dirs::data_dir)
        .ok_or_else(|| "无法确定剪贴板图片缓存目录".to_string())?;
    let dir = base.join("beitang").join("clipboard-images");
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("创建剪贴板图片缓存目录失败 {}: {}", dir.display(), err))?;
    Ok(dir)
}

fn write_clipboard_image_to_dir(
    image: &ClipboardImage,
    directory: &Path,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(directory)
        .map_err(|err| format!("创建剪贴板图片目录失败 {}: {}", directory.display(), err))?;

    let decoded = decode_clipboard_image(image)?;
    let path = directory.join(format!("clipboard-{}.png", Uuid::new_v4()));
    decoded
        .save_with_format(&path, SourceImageFormat::Png)
        .map_err(|err| format!("写入剪贴板图片失败 {}: {}", path.display(), err))?;
    Ok(path)
}

fn prepare_pending_attachment_from_clipboard_image(
    image: &ClipboardImage,
    directory: &Path,
) -> Result<PendingAttachment, String> {
    std::fs::create_dir_all(directory)
        .map_err(|err| format!("创建剪贴板图片目录失败 {}: {}", directory.display(), err))?;

    if let Some((mime_type, extension)) = supported_clipboard_output(image.format()) {
        let bytes = image.bytes().to_vec();
        let (width, height) = read_image_dimensions_from_bytes(&bytes, image.format())?;
        let path = directory.join(format!("clipboard-{}.{}", Uuid::new_v4(), extension));
        std::fs::write(&path, &bytes)
            .map_err(|err| format!("写入剪贴板图片失败 {}: {}", path.display(), err))?;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("clipboard-image")
            .to_string();
        return pending_attachment_from_data(
            path,
            file_name,
            mime_type.to_string(),
            width,
            height,
            bytes,
        );
    }

    let decoded = decode_clipboard_image(image)?;
    let (width, height) = decoded.dimensions();
    let mut cursor = Cursor::new(Vec::new());
    decoded
        .write_to(&mut cursor, SourceImageFormat::Png)
        .map_err(|err| format!("编码剪贴板图片失败: {}", err))?;
    let bytes = cursor.into_inner();
    let path = directory.join(format!("clipboard-{}.png", Uuid::new_v4()));
    std::fs::write(&path, &bytes)
        .map_err(|err| format!("写入剪贴板图片失败 {}: {}", path.display(), err))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("clipboard-image.png")
        .to_string();
    pending_attachment_from_data(
        path,
        file_name,
        "image/png".to_string(),
        width,
        height,
        bytes,
    )
}

fn decode_clipboard_image(image: &ClipboardImage) -> Result<DynamicImage, String> {
    let Some(format) = source_image_format_from_clipboard(image.format()) else {
        return Err(format!(
            "暂不支持从剪贴板导入 {:?} 格式图片",
            image.format()
        ));
    };

    ImageReader::with_format(Cursor::new(image.bytes()), format)
        .decode()
        .map_err(|err| format!("解码剪贴板图片失败: {}", err))
}

fn read_image_dimensions_from_bytes(
    bytes: &[u8],
    format: ClipboardImageFormat,
) -> Result<(u32, u32), String> {
    let Some(source_format) = source_image_format_from_clipboard(format) else {
        return Err(format!("暂不支持从剪贴板导入 {:?} 格式图片", format));
    };

    ImageReader::with_format(Cursor::new(bytes), source_format)
        .into_dimensions()
        .map_err(|err| format!("读取剪贴板图片尺寸失败: {}", err))
}

fn source_image_format_from_clipboard(format: ClipboardImageFormat) -> Option<SourceImageFormat> {
    match format {
        ClipboardImageFormat::Png => Some(SourceImageFormat::Png),
        ClipboardImageFormat::Jpeg => Some(SourceImageFormat::Jpeg),
        ClipboardImageFormat::Webp => Some(SourceImageFormat::WebP),
        ClipboardImageFormat::Gif => Some(SourceImageFormat::Gif),
        ClipboardImageFormat::Bmp => Some(SourceImageFormat::Bmp),
        ClipboardImageFormat::Tiff => Some(SourceImageFormat::Tiff),
        ClipboardImageFormat::Ico => Some(SourceImageFormat::Ico),
        ClipboardImageFormat::Svg => None,
    }
}

fn supported_clipboard_output(
    format: ClipboardImageFormat,
) -> Option<(&'static str, &'static str)> {
    match format {
        ClipboardImageFormat::Png => Some(("image/png", "png")),
        ClipboardImageFormat::Jpeg => Some(("image/jpeg", "jpg")),
        ClipboardImageFormat::Gif => Some(("image/gif", "gif")),
        ClipboardImageFormat::Webp => Some(("image/webp", "webp")),
        _ => None,
    }
}

fn collect_external_image_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut accepted = Vec::new();
    let mut seen_paths = HashSet::new();
    for path in paths {
        if !path.exists() || !path.is_file() {
            continue;
        }
        if read_source_image_metadata(path).is_err() {
            continue;
        }
        if seen_paths.insert(path.clone()) {
            accepted.push(path.clone());
        }
    }
    accepted
}

fn path_looks_like_image(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };

    matches!(
        extension.to_ascii_lowercase().as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "bmp"
            | "tif"
            | "tiff"
            | "ico"
            | "svg"
            | "heic"
            | "heif"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        ClipboardEntry, ClipboardItem, ClipboardString, Image as ClipboardImage,
        ImageFormat as ClipboardImageFormat,
    };
    use image::{ImageBuffer, Rgb};
    use tempfile::TempDir;

    #[test]
    fn clipboard_png_image_becomes_temp_png_and_can_prepare_attachment() {
        let temp_dir = TempDir::new().unwrap();
        let item = ClipboardItem {
            entries: vec![ClipboardEntry::Image(clipboard_image_bytes(
                ClipboardImageFormat::Png,
            ))],
        };

        let paths = extract_image_paths_in_dir(&item, temp_dir.path()).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths[0].extension().and_then(|ext| ext.to_str()),
            Some("png")
        );

        let attachments = crate::ui::attachment_draft::prepare_pending_attachments(paths).unwrap();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].mime_type, "image/png");
        assert!(attachments[0].preview_image.is_some());
    }

    #[test]
    fn clipboard_png_image_can_prepare_pending_attachment_without_rereading_file() {
        let temp_dir = TempDir::new().unwrap();
        let item = ClipboardItem {
            entries: vec![ClipboardEntry::Image(clipboard_image_bytes(
                ClipboardImageFormat::Png,
            ))],
        };

        let attachments =
            prepare_pending_attachments_from_clipboard_in_dir(&item, temp_dir.path()).unwrap();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].mime_type, "image/png");
        assert_eq!(attachments[0].width, 16);
        assert!(attachments[0].preview_image.is_some());
    }

    #[test]
    fn clipboard_bmp_image_is_normalized_to_png() {
        let temp_dir = TempDir::new().unwrap();
        let item = ClipboardItem {
            entries: vec![ClipboardEntry::Image(clipboard_image_bytes(
                ClipboardImageFormat::Bmp,
            ))],
        };

        let paths = extract_image_paths_in_dir(&item, temp_dir.path()).unwrap();
        let metadata = read_source_image_metadata(&paths[0]).unwrap();
        assert_eq!(metadata.mime_type, "image/png");
    }

    #[test]
    fn external_paths_only_keep_supported_images() {
        let temp_dir = TempDir::new().unwrap();
        let image_path = temp_dir.path().join("valid.png");
        let image: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(16, 12, Rgb([120, 30, 200]));
        image.save(&image_path).unwrap();
        let text_path = temp_dir.path().join("note.txt");
        std::fs::write(&text_path, "hello").unwrap();

        let paths = collect_external_image_paths(&[image_path.clone(), text_path.clone()]);
        assert_eq!(paths, vec![image_path]);
    }

    #[test]
    fn plain_text_clipboard_does_not_produce_attachment_paths() {
        let temp_dir = TempDir::new().unwrap();
        let item = ClipboardItem {
            entries: vec![ClipboardEntry::String(ClipboardString::new(
                "hello".to_string(),
            ))],
        };

        let paths = extract_image_paths_in_dir(&item, temp_dir.path()).unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn image_clipboard_is_recognized_as_candidate() {
        let item = ClipboardItem {
            entries: vec![ClipboardEntry::Image(clipboard_image_bytes(
                ClipboardImageFormat::Png,
            ))],
        };

        assert!(clipboard_has_image_candidate(&item));
    }

    #[test]
    fn plain_text_clipboard_is_not_recognized_as_candidate() {
        let item = ClipboardItem {
            entries: vec![ClipboardEntry::String(ClipboardString::new(
                "hello".to_string(),
            ))],
        };

        assert!(!clipboard_has_image_candidate(&item));
    }

    fn clipboard_image_bytes(format: ClipboardImageFormat) -> ClipboardImage {
        let dynamic = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(16, 12, Rgb([20, 40, 60])));
        let mut bytes = Cursor::new(Vec::new());
        dynamic
            .write_to(
                &mut bytes,
                source_image_format_from_clipboard(format).unwrap(),
            )
            .unwrap();
        ClipboardImage::from_bytes(format, bytes.into_inner())
    }
}
