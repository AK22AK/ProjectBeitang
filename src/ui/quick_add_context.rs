use crate::ui::floating_window::{InputMode, QuickAddPresentation};
use crate::ui::sidebar::Panel;

pub fn resolve_quick_add_mode(current_panel: Panel, last_mode: InputMode) -> InputMode {
    match current_panel {
        Panel::Tasks => InputMode::Task,
        Panel::Records => InputMode::Record,
        _ => last_mode,
    }
}

pub fn resolve_quick_add_presentation(main_window_active: bool) -> QuickAddPresentation {
    if main_window_active {
        QuickAddPresentation::Overlay
    } else {
        QuickAddPresentation::Window
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_task_panel_context() {
        assert_eq!(
            resolve_quick_add_mode(Panel::Tasks, InputMode::Record),
            InputMode::Task
        );
    }

    #[test]
    fn uses_record_panel_context() {
        assert_eq!(
            resolve_quick_add_mode(Panel::Records, InputMode::Task),
            InputMode::Record
        );
    }

    #[test]
    fn preserves_last_mode_outside_creation_panels() {
        assert_eq!(
            resolve_quick_add_mode(Panel::Dashboard, InputMode::Task),
            InputMode::Task
        );
        assert_eq!(
            resolve_quick_add_mode(Panel::Search, InputMode::Record),
            InputMode::Record
        );
    }

    #[test]
    fn prefers_overlay_when_main_window_is_active() {
        assert_eq!(
            resolve_quick_add_presentation(true),
            QuickAddPresentation::Overlay
        );
    }

    #[test]
    fn falls_back_to_window_when_main_window_is_inactive() {
        assert_eq!(
            resolve_quick_add_presentation(false),
            QuickAddPresentation::Window
        );
    }
}
