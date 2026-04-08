use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Mutex, OnceLock};

use gpui::Window;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use rfd::AsyncFileDialog;

type FileDialogFuture = Pin<Box<dyn Future<Output = Option<Vec<PathBuf>>> + Send>>;
type SingleFileDialogFuture = Pin<Box<dyn Future<Output = Option<PathBuf>> + Send>>;

static LAST_IMAGE_DIRECTORY: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
static LAST_ARCHIVE_DIRECTORY: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

#[derive(Clone, Copy)]
pub struct ParentWindowHint {
    raw_window_handle: RawWindowHandle,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    raw_display_handle: RawDisplayHandle,
}

impl ParentWindowHint {
    pub fn from_window(window: &Window) -> Option<Self> {
        let raw_window_handle = HasWindowHandle::window_handle(window).ok()?.as_raw();
        let raw_display_handle = HasDisplayHandle::display_handle(window).ok()?.as_raw();
        Some(Self {
            raw_window_handle,
            raw_display_handle,
        })
    }
}

pub fn pick_image_files(parent_window_hint: Option<ParentWindowHint>) -> FileDialogFuture {
    #[cfg(target_os = "macos")]
    {
        macos::pick_image_files(parent_window_hint)
    }

    #[cfg(not(target_os = "macos"))]
    {
        fallback::pick_image_files(parent_window_hint)
    }
}

pub fn pick_archive_file(parent_window_hint: Option<ParentWindowHint>) -> SingleFileDialogFuture {
    let mut dialog = AsyncFileDialog::new().add_filter("Beitang Export", &["zip"]);
    if let Some(directory) = last_archive_directory() {
        dialog = dialog.set_directory(directory);
    }
    if let Some(parent_window_hint) = parent_window_hint {
        dialog = dialog.set_parent(&RawParentWindow(parent_window_hint));
    }

    Box::pin(async move {
        let handle = dialog.pick_file().await?;
        let path = handle.path().to_path_buf();
        update_last_archive_directory(&path);
        Some(path)
    })
}

pub fn save_archive_file(
    parent_window_hint: Option<ParentWindowHint>,
    file_name: &str,
) -> SingleFileDialogFuture {
    let mut dialog = AsyncFileDialog::new()
        .add_filter("Beitang Export", &["zip"])
        .set_file_name(file_name);
    if let Some(directory) = last_archive_directory() {
        dialog = dialog.set_directory(directory);
    }
    if let Some(parent_window_hint) = parent_window_hint {
        dialog = dialog.set_parent(&RawParentWindow(parent_window_hint));
    }

    Box::pin(async move {
        let handle = dialog.save_file().await?;
        let path = handle.path().to_path_buf();
        update_last_archive_directory(&path);
        Some(path)
    })
}

fn last_image_directory() -> Option<PathBuf> {
    LAST_IMAGE_DIRECTORY
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

fn last_archive_directory() -> Option<PathBuf> {
    LAST_ARCHIVE_DIRECTORY
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

fn update_last_image_directory(paths: &[PathBuf]) {
    let Some(parent) = paths
        .first()
        .and_then(|path| path.parent())
        .map(Path::to_path_buf)
    else {
        return;
    };

    if let Ok(mut guard) = LAST_IMAGE_DIRECTORY.get_or_init(|| Mutex::new(None)).lock() {
        *guard = Some(parent);
    }
}

fn update_last_archive_directory(path: &Path) {
    let Some(parent) = path.parent().map(Path::to_path_buf) else {
        return;
    };

    if let Ok(mut guard) = LAST_ARCHIVE_DIRECTORY
        .get_or_init(|| Mutex::new(None))
        .lock()
    {
        *guard = Some(parent);
    }
}

struct RawParentWindow(ParentWindowHint);

impl HasWindowHandle for RawParentWindow {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(self.0.raw_window_handle) })
    }
}

impl HasDisplayHandle for RawParentWindow {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(self.0.raw_display_handle) })
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{
        last_image_directory, update_last_image_directory, FileDialogFuture, ParentWindowHint,
    };
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel, NSView, NSWindow};
    use objc2_foundation::{NSArray, NSString, NSURL};
    use std::path::PathBuf;
    use std::time::Instant;

    use raw_window_handle::RawWindowHandle;

    pub fn pick_image_files(parent_window_hint: Option<ParentWindowHint>) -> FileDialogFuture {
        let requested_at = Instant::now();
        let (tx, rx) = async_channel::bounded(1);

        let Some(mtm) = MainThreadMarker::new() else {
            eprintln!("[perf][file_dialog] pick_image_files called off main thread");
            return Box::pin(async { None });
        };

        let panel = NSOpenPanel::openPanel(mtm);
        configure_panel(&panel, last_image_directory());

        let completion_panel = panel.clone();
        let completion = RcBlock::new(move |response| {
            let selected_paths = if response == NSModalResponseOK {
                collect_paths(&completion_panel)
            } else {
                None
            };

            if let Some(paths) = selected_paths.as_ref() {
                update_last_image_directory(paths);
            }

            eprintln!(
                "[perf][file_dialog] completed in {:?}",
                requested_at.elapsed()
            );
            let _ = tx.send_blocking(selected_paths);
        });

        let show_started_at = Instant::now();
        if let Some(parent_window) = parent_window_hint.and_then(ns_window_from_hint) {
            panel.beginSheetModalForWindow_completionHandler(&parent_window, &completion);
        } else {
            panel.beginWithCompletionHandler(&completion);
        }
        eprintln!(
            "[perf][file_dialog] panel issued in {:?}",
            show_started_at.elapsed()
        );

        Box::pin(async move { rx.recv().await.ok().flatten() })
    }

    #[allow(deprecated)]
    fn configure_panel(panel: &NSOpenPanel, starting_directory: Option<PathBuf>) {
        let extensions = ["png", "jpg", "jpeg", "webp", "gif"]
            .iter()
            .map(|ext| NSString::from_str(ext))
            .collect::<Vec<_>>();
        let allowed_types = NSArray::from_retained_slice(&extensions);

        panel.setAllowedFileTypes(Some(&allowed_types));
        panel.setCanChooseDirectories(false);
        panel.setCanChooseFiles(true);
        panel.setAllowsMultipleSelection(true);
        panel.setTitle(Some(&NSString::from_str("选择图片")));

        if let Some(starting_directory) = starting_directory {
            let path = starting_directory.to_string_lossy();
            let url = NSURL::fileURLWithPath(&NSString::from_str(&path));
            panel.setDirectoryURL(Some(&url));
        }
    }

    fn collect_paths(panel: &NSOpenPanel) -> Option<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for url in panel.URLs().iter() {
            let Some(path) = url.path() else {
                continue;
            };
            paths.push(PathBuf::from(path.to_string()));
        }

        if paths.is_empty() {
            None
        } else {
            Some(paths)
        }
    }

    fn ns_window_from_hint(hint: ParentWindowHint) -> Option<Retained<NSWindow>> {
        match hint.raw_window_handle {
            RawWindowHandle::AppKit(handle) => {
                let view = handle.ns_view.as_ptr() as *mut NSView;
                let view: Retained<NSView> = unsafe { Retained::retain(view)? };
                view.window()
            }
            _ => None,
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod fallback {
    use super::{
        last_image_directory, update_last_image_directory, FileDialogFuture, ParentWindowHint,
        RawParentWindow,
    };
    use rfd::AsyncFileDialog;
    use std::path::PathBuf;

    pub fn pick_image_files(parent_window_hint: Option<ParentWindowHint>) -> FileDialogFuture {
        let mut dialog =
            AsyncFileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif"]);

        if let Some(directory) = last_image_directory() {
            dialog = dialog.set_directory(directory);
        }

        if let Some(parent_window_hint) = parent_window_hint {
            dialog = dialog.set_parent(&RawParentWindow(parent_window_hint));
        }

        Box::pin(async move {
            let handles = dialog.pick_files().await?;
            let paths: Vec<PathBuf> = handles
                .into_iter()
                .map(|handle| handle.path().to_path_buf())
                .collect();
            update_last_image_directory(&paths);
            Some(paths)
        })
    }
}
