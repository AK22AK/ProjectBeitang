pub mod app_shortcuts;
pub mod attachment_image;
pub mod config;
pub mod db;
pub mod file_dialog;
pub mod file_dialog_prewarm;
pub mod models;
pub mod notifier;
pub mod shortcut_manager;
pub mod store;
pub mod ui;

#[cfg(test)]
mod tests {
    use crate::ui::attachment_draft::PendingAttachment;
    use crate::ui::floating_window::QuickAddSessionController;
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
}
