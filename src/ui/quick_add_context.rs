use crate::ui::floating_window::InputMode;
use crate::ui::sidebar::Panel;

pub fn resolve_quick_add_mode(current_panel: Panel, last_mode: InputMode) -> InputMode {
    match current_panel {
        Panel::Tasks => InputMode::Task,
        Panel::Records => InputMode::Record,
        _ => last_mode,
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
}
