use beitang::config::{parse_hotkey, ShortcutConfig};
use beitang::models::{Priority, Record};
use beitang::shortcut_manager::ShortcutEvent;

#[test]
fn test_default_shortcut_config() {
    let config = ShortcutConfig::default();
    assert_eq!(config.quick_capture, "Cmd+Shift+T");
    assert_eq!(config.open_main, "Cmd+0");
    assert_eq!(config.open_tasks, "Cmd+2");
    assert_eq!(config.open_records, "Cmd+3");
}

#[test]
fn test_shortcut_config_load_returns_default() {
    let config = ShortcutConfig::load();
    assert_eq!(config.quick_capture, "Cmd+Shift+T");
    assert_eq!(config.open_main, "Cmd+0");
}

#[test]
fn test_parse_hotkey_supports_default_shortcuts() {
    assert!(parse_hotkey("Cmd+Shift+T").is_ok());
    assert!(parse_hotkey("Cmd+0").is_ok());
    assert!(parse_hotkey("Cmd+2").is_ok());
    assert!(parse_hotkey("Cmd+3").is_ok());
}

#[test]
fn test_parse_hotkey_rejects_invalid_shortcuts() {
    assert!(parse_hotkey("Cmd+Shift").is_err());
    assert!(parse_hotkey("Cmd+Shift+T+Y").is_err());
    assert!(parse_hotkey("Cmd+Space").is_err());
}

#[test]
fn test_shortcut_event_equality() {
    assert_eq!(ShortcutEvent::QuickCapture, ShortcutEvent::QuickCapture);
    assert_ne!(ShortcutEvent::QuickCapture, ShortcutEvent::OpenMain);
}

#[test]
fn test_shortcut_event_clone() {
    let event = ShortcutEvent::OpenTasks;
    let cloned = event;
    assert_eq!(event, cloned);
}

#[test]
fn test_shortcut_event_variants() {
    let events = vec![
        ShortcutEvent::QuickCapture,
        ShortcutEvent::OpenMain,
        ShortcutEvent::OpenTasks,
        ShortcutEvent::OpenRecords,
    ];

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

#[test]
fn test_new_note_creates_note_without_priority() {
    let note = Record::new_note("Test note".to_string());
    assert_eq!(note.content, "Test note");
    assert_eq!(note.priority, None);
    assert_eq!(note.record_type, beitang::models::RecordType::Note);
}

#[test]
fn test_new_task_creates_task_with_priority() {
    let task = Record::new_task(
        "Test Title".to_string(),
        "Test content".to_string(),
        Priority::High,
    );
    assert_eq!(task.title, Some("Test Title".to_string()));
    assert_eq!(task.content, "Test content");
    assert_eq!(task.priority, Some(Priority::High));
    assert_eq!(task.record_type, beitang::models::RecordType::Task);
}

#[test]
fn test_new_task_with_different_priorities() {
    let high_task = Record::new_task("High task".to_string(), "".to_string(), Priority::High);
    let medium_task = Record::new_task("Medium task".to_string(), "".to_string(), Priority::Medium);
    let low_task = Record::new_task("Low task".to_string(), "".to_string(), Priority::Low);

    assert_eq!(high_task.priority, Some(Priority::High));
    assert_eq!(medium_task.priority, Some(Priority::Medium));
    assert_eq!(low_task.priority, Some(Priority::Low));
}
