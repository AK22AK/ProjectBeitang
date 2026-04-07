use std::sync::Once;

static PREWARM_ONCE: Once = Once::new();

pub fn prewarm_file_dialog() {
    PREWARM_ONCE.call_once(prewarm_file_dialog_inner);
}

#[cfg(target_os = "macos")]
fn prewarm_file_dialog_inner() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSOpenPanel;
    use objc2_foundation::NSThread;

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };

    let thread = NSThread::new();
    thread.start();

    let _ = NSOpenPanel::openPanel(mtm);
}

#[cfg(not(target_os = "macos"))]
fn prewarm_file_dialog_inner() {}
