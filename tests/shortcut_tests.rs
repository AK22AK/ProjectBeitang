use gpui::px;
use robinne::app_shortcuts::{app_shortcut_entries, main_panel_shortcuts};
use robinne::config::{parse_hotkey, ShortcutConfig};
use robinne::models::{Priority, Record};
use robinne::shortcut_manager::ShortcutEvent;
use robinne::ui::sidebar::{
    main_sidebar_layout_mode, main_sidebar_width, Panel, SidebarLayoutMode,
};

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
fn test_app_shortcut_entries_are_not_in_global_shortcut_config() {
    let config_labels = ShortcutConfig::default()
        .entries()
        .into_iter()
        .map(|(label, _)| label)
        .collect::<Vec<_>>();

    assert!(!config_labels.contains(&"搜索"));
    assert!(!config_labels.contains(&"设置"));
    assert_eq!(
        app_shortcut_entries(),
        [("快速创建", "Cmd+N"), ("搜索", "Cmd+K"), ("设置", "Cmd+,")]
    );
}

#[test]
fn test_main_panel_shortcuts_exclude_bottom_sidebar_panels() {
    assert_eq!(
        main_panel_shortcuts(),
        [
            ("1", Panel::Dashboard),
            ("2", Panel::Tasks),
            ("3", Panel::Records),
            ("4", Panel::Timeline),
            ("5", Panel::AI),
        ]
    );

    assert!(main_panel_shortcuts()
        .into_iter()
        .all(|(_, panel)| !matches!(panel, Panel::Search | Panel::Settings)));
}

#[test]
fn test_main_sidebar_layout_mode_breakpoint() {
    assert_eq!(
        main_sidebar_layout_mode(px(840.0)),
        SidebarLayoutMode::Expanded
    );
    assert_eq!(
        main_sidebar_layout_mode(px(839.0)),
        SidebarLayoutMode::Compact
    );
}

#[test]
fn test_main_sidebar_width_by_layout_mode() {
    assert_eq!(main_sidebar_width(SidebarLayoutMode::Expanded), px(200.0));
    assert_eq!(main_sidebar_width(SidebarLayoutMode::Compact), px(64.0));
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
    assert_eq!(note.record_type, robinne::models::RecordType::Note);
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
    assert_eq!(task.record_type, robinne::models::RecordType::Task);
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
