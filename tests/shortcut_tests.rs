use gpui::{px, Keystroke, Modifiers};
use robinne::app_shortcuts::{app_shortcut_entries, main_panel_shortcuts};
use robinne::config::{
    format_shortcut_for_display, keystroke_matches_shortcut, parse_hotkey, shortcut_from_keystroke,
    ShortcutConfig,
};
use robinne::models::{Priority, Record};
use robinne::platform::{
    app_shortcut_keystrokes, default_global_shortcuts, primary_modifier_label,
};
use robinne::shortcut_manager::ShortcutEvent;
use robinne::ui::floating_window::{
    should_hide_app_after_global_quick_add_launch, should_hide_app_after_quick_add_close,
};
use robinne::ui::sidebar::{
    main_sidebar_layout_mode, main_sidebar_width, Panel, SidebarLayoutMode,
};

#[test]
fn test_default_shortcut_config() {
    let config = ShortcutConfig::default();
    let defaults = default_global_shortcuts();
    assert_eq!(config.quick_capture, defaults.quick_capture);
    assert_eq!(config.open_main, defaults.open_main);
    assert_eq!(config.open_tasks, defaults.open_tasks);
    assert_eq!(config.open_records, defaults.open_records);
}

#[test]
fn test_shortcut_config_load_returns_valid_config() {
    let config = ShortcutConfig::load();
    assert!(parse_hotkey(&config.quick_capture).is_ok());
    assert!(parse_hotkey(&config.open_main).is_ok());
}

#[test]
fn test_parse_hotkey_supports_default_shortcuts() {
    let defaults = default_global_shortcuts();
    assert!(parse_hotkey(defaults.quick_capture).is_ok());
    assert!(parse_hotkey(defaults.open_main).is_ok());
    assert!(parse_hotkey(defaults.open_tasks).is_ok());
    assert!(parse_hotkey(defaults.open_records).is_ok());
}

#[test]
fn test_parse_hotkey_rejects_invalid_shortcuts() {
    assert!(parse_hotkey("Cmd+Shift").is_err());
    assert!(parse_hotkey("Cmd+Shift+T+Y").is_err());
    assert!(parse_hotkey("Cmd+Space").is_err());
}

#[test]
fn test_format_shortcut_for_display_normalizes_tokens() {
    assert_eq!(
        format_shortcut_for_display("cmd+shift+t").unwrap(),
        "Cmd+Shift+T"
    );
    assert_eq!(format_shortcut_for_display("ctrl+2").unwrap(), "Ctrl+2");
}

#[test]
fn test_shortcut_from_keystroke_formats_modifier_combo() {
    let keystroke = Keystroke {
        modifiers: Modifiers {
            control: true,
            shift: true,
            ..Default::default()
        },
        key: "t".to_string(),
        key_char: None,
    };

    assert_eq!(
        shortcut_from_keystroke(&keystroke).unwrap(),
        Some("Ctrl+Shift+T".to_string())
    );
}

#[test]
fn test_shortcut_from_keystroke_ignores_modifier_only_input() {
    let keystroke = Keystroke {
        modifiers: Modifiers {
            shift: true,
            ..Default::default()
        },
        key: "shift".to_string(),
        key_char: None,
    };

    assert_eq!(shortcut_from_keystroke(&keystroke).unwrap(), None);
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
    let keystrokes = app_shortcut_keystrokes();
    if primary_modifier_label() == "Cmd" {
        assert_eq!(keystrokes.quick_add_overlay, "cmd-n");
        assert_eq!(
            app_shortcut_entries(),
            [("快速创建", "Cmd+N"), ("搜索", "Cmd+K"), ("设置", "Cmd+,")]
        );
    } else {
        assert_eq!(keystrokes.quick_add_overlay, "ctrl-n");
        assert_eq!(
            app_shortcut_entries(),
            [
                ("快速创建", "Ctrl+N"),
                ("搜索", "Ctrl+K"),
                ("设置", "Ctrl+,")
            ]
        );
    }
}

#[test]
fn test_main_panel_shortcuts_exclude_bottom_sidebar_panels() {
    assert_eq!(
        main_panel_shortcuts(),
        [
            ("1", Panel::Dashboard),
            ("4", Panel::Timeline),
            ("5", Panel::AI),
        ]
    );

    assert!(main_panel_shortcuts()
        .into_iter()
        .all(|(_, panel)| !matches!(panel, Panel::Search | Panel::Settings)));
}

#[test]
fn test_keystroke_matches_shortcut_uses_normalized_tokens() {
    let keystroke = Keystroke {
        modifiers: Modifiers {
            platform: true,
            shift: true,
            ..Default::default()
        },
        key: "u".to_string(),
        key_char: None,
    };

    assert!(keystroke_matches_shortcut(&keystroke, "cmd+shift+u"));
    assert!(!keystroke_matches_shortcut(&keystroke, "cmd+shift+t"));
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
fn test_global_quick_add_only_hides_app_when_no_other_window_exists() {
    assert!(should_hide_app_after_global_quick_add_launch(false, false));
    assert!(should_hide_app_after_global_quick_add_launch(false, true));
    assert!(!should_hide_app_after_global_quick_add_launch(true, false));
}

#[test]
fn test_quick_add_close_only_hides_when_it_is_the_last_window() {
    assert!(should_hide_app_after_quick_add_close(true, 1));
    assert!(!should_hide_app_after_quick_add_close(true, 2));
    assert!(!should_hide_app_after_quick_add_close(false, 1));
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
