pub mod app_shortcuts;
pub mod attachment_image;
pub mod clipboard_attachment;
pub mod config;
pub mod data_management;
pub mod db;
pub mod git_sync;
pub mod models;
pub mod platform;
pub mod settings;
pub mod shortcut_manager;
pub mod store;
pub mod ui;

#[cfg(test)]
mod tests {
    use crate::ui::attachment_draft::PendingAttachment;
    use crate::ui::floating_window::{QuickAddPresentation, QuickAddSessionController};
    use std::path::PathBuf;

    fn test_pending_attachment() -> PendingAttachment {
        PendingAttachment {
            path: PathBuf::from("/tmp/test.png"),
            file_name: "test.png".to_string(),
            file_size: 128,
            mime_type: "image/png".to_string(),
            width: 16,
            height: 16,
            preview_image: None,
        }
    }

    #[test]
    fn quick_add_session_has_draft_for_text_or_attachments() {
        let mut session = QuickAddSessionController::default();
        assert!(!session.has_draft());

        session.draft_text = "hello".to_string();
        assert!(session.has_draft());

        session.draft_text.clear();
        session.pending_attachments.push(test_pending_attachment());
        assert!(session.has_draft());

        session.clear();
        assert!(!session.has_draft());
        assert!(session.pending_attachments.is_empty());
    }

    #[test]
    fn quick_add_restore_visible_does_not_advance_request_serial() {
        let mut session = QuickAddSessionController::default();
        let request_serial = session.mark_visible(QuickAddPresentation::Window);

        session.restore_visible(QuickAddPresentation::Window);

        assert_eq!(session.request_serial, request_serial);
        assert_eq!(session.presentation, Some(QuickAddPresentation::Window));
    }
}
