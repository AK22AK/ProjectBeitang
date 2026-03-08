use beitang::config::ShortcutConfig;
use beitang::models::{Priority, Record};
use beitang::shortcut_manager::ShortcutEvent;

// ========== Shortcut Configuration Tests ==========

#[test]
fn test_default_shortcut_config() {
    let config = ShortcutConfig::default();
    assert_eq!(config.quick_add_task, "Cmd+N");
    assert_eq!(config.quick_add_note, "Cmd+M");
    assert_eq!(config.view_tasks, "Cmd+1");
    assert_eq!(config.view_notes, "Cmd+2");
    assert_eq!(config.open_main, "Cmd+0");
}

#[test]
fn test_shortcut_config_load_returns_default() {
    let config = ShortcutConfig::load();
    // Currently load() just returns default
    assert_eq!(config.quick_add_task, "Cmd+N");
    assert_eq!(config.quick_add_note, "Cmd+M");
}

// ========== Shortcut Event Tests ==========

#[test]
fn test_shortcut_event_equality() {
    assert_eq!(ShortcutEvent::QuickAddTask, ShortcutEvent::QuickAddTask);
    assert_ne!(ShortcutEvent::QuickAddTask, ShortcutEvent::QuickAddNote);
}

#[test]
fn test_shortcut_event_clone() {
    let event = ShortcutEvent::ViewTasks;
    let cloned = event;
    assert_eq!(event, cloned);
}

#[test]
fn test_shortcut_event_variants() {
    // Test all event variants exist and can be compared
    let events = vec![
        ShortcutEvent::QuickAddTask,
        ShortcutEvent::QuickAddNote,
        ShortcutEvent::ViewTasks,
        ShortcutEvent::ViewNotes,
        ShortcutEvent::OpenMain,
    ];

    // Ensure all events are unique
    for (i, event) in events.iter().enumerate() {
        for (j, other) in events.iter().enumerate() {
            if i == j {
                assert_eq!(event, other);
            } else {
                assert_ne!(event, other);
            }
        }
    }
}

// ========== Record Creation Tests ==========

#[test]
fn test_new_note_creates_note_without_priority() {
    let note = Record::new_note("Test note".to_string());
    assert_eq!(note.content, "Test note");
    assert_eq!(note.priority, None);
    assert_eq!(note.record_type, beitang::models::RecordType::Note);
}

#[test]
fn test_new_task_creates_task_with_priority() {
    let task = Record::new_task("Test task".to_string(), Priority::High);
    assert_eq!(task.content, "Test task");
    assert_eq!(task.priority, Some(Priority::High));
    assert_eq!(task.record_type, beitang::models::RecordType::Task);
}

#[test]
fn test_new_task_with_different_priorities() {
    let high_task = Record::new_task("High task".to_string(), Priority::High);
    let medium_task = Record::new_task("Medium task".to_string(), Priority::Medium);
    let low_task = Record::new_task("Low task".to_string(), Priority::Low);

    assert_eq!(high_task.priority, Some(Priority::High));
    assert_eq!(medium_task.priority, Some(Priority::Medium));
    assert_eq!(low_task.priority, Some(Priority::Low));
}
