use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::scroll::ScrollableElement;
use gpui_component::{h_flex, ActiveTheme, IconName, Sizable, TitleBar};
use gpui_component_assets::Assets;
use gpui_platform::application;
use robinne::app_shortcuts::{
    app_shortcut_entries, main_panel_shortcuts, quick_add_overlay_keystroke, search_keystroke,
    settings_keystroke,
};
use robinne::config::ShortcutConfig;
use robinne::platform::{
    app_shortcut_scope_description, app_shortcuts_intro, build_app_menus,
    global_shortcut_scope_description, prewarm_file_dialog,
};
use robinne::store::{create_store, Store};
use robinne::ui::dashboard::{Dashboard, DashboardAction};
use robinne::ui::data_management::DataManagementPanel;
use robinne::ui::floating_window::{
    quick_add_window_size, should_hide_app_after_global_quick_add_launch,
    should_hide_app_after_quick_add_close, InputMode, QuickAddDestination, QuickAddPresentation,
    QuickAddSessionController, QuickAddSessionStatus, QuickAddWindow,
};
use robinne::ui::note_panel::NotePanel;
use robinne::ui::quick_add_context::resolve_quick_add_mode;
use robinne::ui::search::SearchPanel;
use robinne::ui::sidebar::{main_sidebar_layout_mode, main_sidebar_width, Panel, Sidebar};
use robinne::ui::task_panel::TaskPanel;
use robinne::ui::timeline::Timeline;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

const SETTINGS_NAV_BREAKPOINT: Pixels = px(600.0);
const SETTINGS_SIDEBAR_NAV_WIDTH: Pixels = px(180.0);
const QUICK_CAPTURE_DEBOUNCE_WINDOW: std::time::Duration = std::time::Duration::from_millis(250);

actions!(
    app_menu,
    [OpenQuickAddOverlay, OpenSearch, OpenSettings, QuitApp]
);

#[derive(Clone)]
struct MainWindowController {
    handle: Option<AnyWindowHandle>,
    window_id: Option<WindowId>,
    current_panel: Panel,
    is_active: bool,
}

impl Default for MainWindowController {
    fn default() -> Self {
        Self {
            handle: None,
            window_id: None,
            current_panel: Panel::Dashboard,
            is_active: false,
        }
    }
}

impl MainWindowController {
    fn track(&mut self, handle: AnyWindowHandle, panel: Panel) {
        self.handle = Some(handle);
        self.window_id = Some(handle.window_id());
        self.current_panel = panel;
        self.is_active = true;
    }

    fn clear_handle(&mut self) {
        self.handle = None;
        self.window_id = None;
        self.is_active = false;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsSection {
    DataManagement,
    General,
    Shortcuts,
    About,
}

impl SettingsSection {
    fn label(self) -> &'static str {
        match self {
            Self::DataManagement => "数据管理",
            Self::General => "通用",
            Self::Shortcuts => "快捷键",
            Self::About => "关于",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::DataManagement => "数据管理",
            Self::General => "通用",
            Self::Shortcuts => "快捷键",
            Self::About => "关于",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::DataManagement => "本地数据统计、附件健康状态和导入导出能力统一放在这里。",
            Self::General => "应用级偏好、显示方式等通用设置将在这里集中管理。",
            Self::Shortcuts => "这里区分展示应用内快捷键和全局快捷键，避免混淆触发范围。",
            Self::About => "版本信息、更新说明和相关说明将在这里统一展示。",
        }
    }

    fn all() -> [Self; 4] {
        [
            Self::DataManagement,
            Self::General,
            Self::Shortcuts,
            Self::About,
        ]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsLayoutMode {
    Sidebar,
    TopTabs,
}

fn install_app_shortcuts_and_menus(
    cx: &mut App,
    main_window: Rc<RefCell<MainWindowController>>,
    store: Store,
    quick_add_session: Rc<RefCell<QuickAddSessionController>>,
) {
    cx.bind_keys([
        KeyBinding::new(quick_add_overlay_keystroke(), OpenQuickAddOverlay, None),
        KeyBinding::new(search_keystroke(), OpenSearch, None),
        KeyBinding::new(settings_keystroke(), OpenSettings, None),
    ]);

    let main_window_for_quick_add = main_window.clone();
    let store_for_quick_add = store.clone();
    let quick_add_for_quick_add = quick_add_session.clone();
    cx.on_action(move |_: &OpenQuickAddOverlay, cx| {
        let main_window = main_window_for_quick_add.clone();
        let store = store_for_quick_add.clone();
        let quick_add_session = quick_add_for_quick_add.clone();
        cx.defer(move |cx| {
            open_or_focus_quick_add_overlay(cx, store, main_window, quick_add_session);
        });
    });

    let main_window_for_search = main_window.clone();
    let store_for_search = store.clone();
    let quick_add_for_search = quick_add_session.clone();
    cx.on_action(move |_: &OpenSearch, cx| {
        let main_window = main_window_for_search.clone();
        let store = store_for_search.clone();
        let quick_add_session = quick_add_for_search.clone();
        // Defer to avoid updating the active window while the action is being dispatched from it.
        cx.defer(move |cx| {
            ensure_main_window(
                cx,
                &main_window,
                &store,
                &quick_add_session,
                Some(Panel::Search),
            );
        });
    });

    let main_window_for_settings = main_window.clone();
    let store_for_settings = store.clone();
    let quick_add_for_settings = quick_add_session.clone();
    cx.on_action(move |_: &OpenSettings, cx| {
        let main_window = main_window_for_settings.clone();
        let store = store_for_settings.clone();
        let quick_add_session = quick_add_for_settings.clone();
        // Defer to avoid updating the active window while the action is being dispatched from it.
        cx.defer(move |cx| {
            ensure_main_window(
                cx,
                &main_window,
                &store,
                &quick_add_session,
                Some(Panel::Settings),
            );
        });
    });

    cx.on_action(|_: &QuitApp, cx| cx.quit());

    cx.set_menus(build_app_menus(
        OpenQuickAddOverlay,
        OpenSearch,
        OpenSettings,
        QuitApp,
    ));
}

fn main() {
    let app = application().with_assets(Assets);

    let (store, mut runtime) = create_store();
    let shortcuts = ShortcutConfig::load();

    let main_window = Rc::new(RefCell::new(MainWindowController::default()));
    let quick_add_session = Rc::new(RefCell::new(QuickAddSessionController::default()));

    let main_window_for_reopen = main_window.clone();
    let store_for_reopen = store.clone();
    let quick_add_for_reopen = quick_add_session.clone();
    app.on_reopen(move |cx| {
        ensure_main_window(
            cx,
            &main_window_for_reopen,
            &store_for_reopen,
            &quick_add_for_reopen,
            None,
        );
    });

    let main_window_for_run = main_window.clone();
    let quick_add_for_run = quick_add_session.clone();
    let store_for_run = store.clone();
    app.run(move |cx| {
        gpui_component::init(cx);
        gpui_component::Theme::change(gpui_component::ThemeMode::Light, None, cx);
        install_app_shortcuts_and_menus(
            cx,
            main_window_for_run.clone(),
            store_for_run.clone(),
            quick_add_for_run.clone(),
        );
        let main_window_for_closed = main_window_for_run.clone();
        cx.on_window_closed(move |cx| {
            sync_main_window_controller(cx, &main_window_for_closed);
        })
        .detach();

        cx.spawn(|_cx: &mut AsyncApp| async move {
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("robinne");
            std::fs::create_dir_all(&data_dir).ok();

            let db_path = data_dir.join("data.db");
            runtime.run(db_path).await;
        })
        .detach();

        cx.defer(|_cx| {
            prewarm_file_dialog();
        });

        if let Ok(manager) = GlobalHotKeyManager::new() {
            let quick_capture = shortcuts.quick_capture_hotkey();
            let open_main = shortcuts.open_main_hotkey();
            let open_tasks = shortcuts.open_tasks_hotkey();
            let open_records = shortcuts.open_records_hotkey();

            match (quick_capture, open_main, open_tasks, open_records) {
                (Ok(quick_capture), Ok(open_main), Ok(open_tasks), Ok(open_records)) => {
                    let registrations = [
                        (quick_capture, shortcuts.quick_capture.as_str()),
                        (open_main, shortcuts.open_main.as_str()),
                        (open_tasks, shortcuts.open_tasks.as_str()),
                        (open_records, shortcuts.open_records.as_str()),
                    ];

                    let mut failed = false;
                    for (hotkey, label) in registrations {
                        if let Err(err) = manager.register(hotkey) {
                            failed = true;
                            eprintln!("[Global Hotkey] Failed to register {}: {}", label, err);
                        } else {
                            eprintln!("[Global Hotkey] Registered {}", label);
                        }
                    }

                    if !failed {
                        let store_for_hotkey = store_for_run.clone();
                        let main_window_for_hotkey = main_window_for_run.clone();
                        let quick_add_for_hotkey = quick_add_for_run.clone();

                        cx.spawn(async move |cx| {
                            let _manager = manager;
                            let receiver = GlobalHotKeyEvent::receiver();

                            loop {
                                if let Ok(event) = receiver.try_recv() {
                                    if event.state == HotKeyState::Released {
                                        cx.update(|cx| {
                                            if event.id == quick_capture.id() {
                                                handle_quick_capture_hotkey(
                                                    cx,
                                                    store_for_hotkey.clone(),
                                                    main_window_for_hotkey.clone(),
                                                    quick_add_for_hotkey.clone(),
                                                );
                                            } else if event.id == open_main.id() {
                                                ensure_main_window(
                                                    cx,
                                                    &main_window_for_hotkey,
                                                    &store_for_hotkey,
                                                    &quick_add_for_hotkey,
                                                    None,
                                                );
                                            } else if event.id == open_tasks.id() {
                                                ensure_main_window(
                                                    cx,
                                                    &main_window_for_hotkey,
                                                    &store_for_hotkey,
                                                    &quick_add_for_hotkey,
                                                    Some(Panel::Tasks),
                                                );
                                            } else if event.id == open_records.id() {
                                                ensure_main_window(
                                                    cx,
                                                    &main_window_for_hotkey,
                                                    &store_for_hotkey,
                                                    &quick_add_for_hotkey,
                                                    Some(Panel::Records),
                                                );
                                            }
                                        });
                                    }
                                }

                                cx.background_executor()
                                    .timer(std::time::Duration::from_millis(100))
                                    .await;
                            }
                        })
                        .detach();
                    }
                }
                _ => {
                    eprintln!("[Global Hotkey] Failed to parse shortcut config");
                }
            }
        } else {
            eprintln!("[Global Hotkey] Initialization failed!");
        }
    });
}

fn handle_quick_capture_hotkey(
    cx: &mut App,
    store: Store,
    main_window: Rc<RefCell<MainWindowController>>,
    quick_add_session: Rc<RefCell<QuickAddSessionController>>,
) {
    let now = std::time::Instant::now();
    {
        let mut session = quick_add_session.borrow_mut();
        if session
            .last_hotkey_at
            .is_some_and(|last| now.duration_since(last) < QUICK_CAPTURE_DEBOUNCE_WINDOW)
        {
            return;
        }
        session.last_hotkey_at = Some(now);
    }

    dismiss_quick_add_overlay_for_window(cx, &main_window, &quick_add_session);

    let (status, current_presentation, has_draft) = {
        let session = quick_add_session.borrow();
        (session.status, session.presentation, session.has_draft())
    };

    match status {
        QuickAddSessionStatus::Closed => {
            let activate_app = cx.active_window().is_none();
            let hide_app_on_close = should_hide_app_after_global_quick_add_launch(
                cx.active_window().is_some(),
                main_window_exists(cx, &main_window),
            );
            open_or_focus_quick_add_window_only(
                cx,
                store,
                main_window,
                quick_add_session,
                None,
                activate_app,
                hide_app_on_close,
            );
        }
        QuickAddSessionStatus::Dormant => {
            let activate_app = cx.active_window().is_none();
            open_or_focus_quick_add_window_only(
                cx,
                store,
                main_window,
                quick_add_session,
                None,
                activate_app,
                false,
            );
        }
        QuickAddSessionStatus::Visible => {
            if current_presentation == Some(QuickAddPresentation::Window) {
                if has_draft {
                    show_quick_add_hotkey_protection(cx, &main_window, &quick_add_session);
                } else {
                    close_visible_quick_add(cx, &main_window, &quick_add_session);
                }
            } else {
                open_or_focus_quick_add_window_only(
                    cx,
                    store,
                    main_window,
                    quick_add_session,
                    None,
                    cx.active_window().is_none(),
                    false,
                );
            }
        }
    }
}

fn dismiss_quick_add_window_for_overlay(
    cx: &mut App,
    quick_add_session: &Rc<RefCell<QuickAddSessionController>>,
) {
    let handle = {
        let session = quick_add_session.borrow();
        if session.presentation == Some(QuickAddPresentation::Window) {
            session.handle
        } else {
            None
        }
    };

    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window, _| {
            window.remove_window();
        });
    }

    let mut session = quick_add_session.borrow_mut();
    if session.presentation == Some(QuickAddPresentation::Window) {
        session.handle = None;
        session.presentation = None;
        session.status = if session.has_draft() {
            QuickAddSessionStatus::Dormant
        } else {
            QuickAddSessionStatus::Closed
        };
        session.hide_app_on_close = false;
    }
}

fn dismiss_quick_add_overlay_for_window(
    cx: &mut App,
    main_window: &Rc<RefCell<MainWindowController>>,
    quick_add_session: &Rc<RefCell<QuickAddSessionController>>,
) {
    let presentation = quick_add_session.borrow().presentation;
    if presentation != Some(QuickAddPresentation::Overlay) {
        return;
    }

    let _ = dismiss_quick_add_overlay(cx, main_window);

    let mut session = quick_add_session.borrow_mut();
    if session.presentation == Some(QuickAddPresentation::Overlay) {
        session.presentation = None;
        session.status = if session.has_draft() {
            QuickAddSessionStatus::Dormant
        } else {
            QuickAddSessionStatus::Closed
        };
        session.hide_app_on_close = false;
    }
}

fn prime_quick_add_session(
    quick_add_session: &Rc<RefCell<QuickAddSessionController>>,
    preferred_mode: Option<InputMode>,
    hide_app_on_close: bool,
) -> u64 {
    let mut session = quick_add_session.borrow_mut();
    if let Some(mode) = preferred_mode {
        session.mode = mode;
    }
    let request_serial = session.mark_visible(QuickAddPresentation::Window);
    session.handle = None;
    session.hide_app_on_close = hide_app_on_close;
    request_serial
}

fn destroy_inactive_main_window_if_needed(
    cx: &mut App,
    controller: &Rc<RefCell<MainWindowController>>,
) {
    let Some(handle) = resolve_main_window_handle(cx, controller) else {
        return;
    };

    if controller.borrow().is_active {
        return;
    }

    let _ = handle.update(cx, |_, window, _| {
        window.remove_window();
    });
    controller.borrow_mut().clear_handle();
}

fn focus_visible_quick_add(
    cx: &mut App,
    controller: &Rc<RefCell<MainWindowController>>,
    quick_add_session: &Rc<RefCell<QuickAddSessionController>>,
) -> bool {
    let presentation = quick_add_session.borrow().presentation;
    match presentation {
        Some(QuickAddPresentation::Window) => {
            let handle = quick_add_session.borrow().handle;
            let Some(handle) = handle else {
                return false;
            };

            handle
                .update(cx, |_, window, _| {
                    window.activate_window();
                })
                .is_ok()
        }
        Some(QuickAddPresentation::Overlay) => focus_quick_add_overlay(cx, controller),
        None => false,
    }
}

fn open_or_focus_quick_add_window_only(
    cx: &mut App,
    store: Store,
    main_window: Rc<RefCell<MainWindowController>>,
    quick_add_session: Rc<RefCell<QuickAddSessionController>>,
    preferred_mode: Option<InputMode>,
    activate_app: bool,
    hide_app_on_close: bool,
) {
    if quick_add_session.borrow().presentation == Some(QuickAddPresentation::Window)
        && focus_visible_quick_add(cx, &main_window, &quick_add_session)
    {
        return;
    }

    open_quick_add_window(
        cx,
        store,
        main_window,
        quick_add_session,
        activate_app,
        hide_app_on_close,
        preferred_mode,
    );
}

fn open_or_focus_quick_add_overlay(
    cx: &mut App,
    store: Store,
    main_window: Rc<RefCell<MainWindowController>>,
    quick_add_session: Rc<RefCell<QuickAddSessionController>>,
) {
    let preferred_mode = {
        let panel = main_window.borrow().current_panel;
        let last_mode = quick_add_session.borrow().mode;
        resolve_quick_add_mode(panel, last_mode)
    };

    ensure_main_window(cx, &main_window, &store, &quick_add_session, None);
    dismiss_quick_add_window_for_overlay(cx, &quick_add_session);
    let _ = show_quick_add_overlay(cx, &main_window, Some(preferred_mode));
}

fn open_quick_add_window(
    cx: &mut App,
    store: Store,
    main_window: Rc<RefCell<MainWindowController>>,
    quick_add_session: Rc<RefCell<QuickAddSessionController>>,
    activate_app: bool,
    hide_app_on_close: bool,
    preferred_mode: Option<InputMode>,
) {
    if activate_app && main_window_exists(cx, &main_window) {
        destroy_inactive_main_window_if_needed(cx, &main_window);
    }

    let request_serial =
        prime_quick_add_session(&quick_add_session, preferred_mode, hide_app_on_close);

    // Showing a floating quick-add window from a background app still requires app activation.
    // Keep this independent from whether the app should be hidden again on close.
    if activate_app {
        cx.activate(true);
    }

    let window_size = quick_add_window_size();
    let window_bounds = WindowBounds::Windowed(Bounds::centered(None, window_size, cx));

    let open_destination: Arc<dyn Fn(QuickAddDestination, &mut App)> = {
        let store = store.clone();
        let main_window = main_window.clone();
        let quick_add_session = quick_add_session.clone();
        Arc::new(move |destination, cx| match destination {
            QuickAddDestination::Main => {
                ensure_main_window(cx, &main_window, &store, &quick_add_session, None);
            }
            QuickAddDestination::Tasks => {
                ensure_main_window(
                    cx,
                    &main_window,
                    &store,
                    &quick_add_session,
                    Some(Panel::Tasks),
                );
            }
            QuickAddDestination::Records => {
                ensure_main_window(
                    cx,
                    &main_window,
                    &store,
                    &quick_add_session,
                    Some(Panel::Records),
                );
            }
        })
    };

    let session_for_window = quick_add_session.clone();
    let open_destination_for_window = open_destination.clone();
    let store_for_window = store.clone();

    match cx.open_window(
        WindowOptions {
            window_bounds: Some(window_bounds),
            kind: WindowKind::Floating,
            is_resizable: false,
            ..Default::default()
        },
        move |window, cx| {
            let session = session_for_window.clone();
            let open_destination = open_destination_for_window.clone();
            let store = store_for_window.clone();
            let view = cx.new(|cx| {
                let mut view =
                    QuickAddWindow::new_window(store, session, open_destination, window, cx);
                view.hide_app_on_close = hide_app_on_close;
                view
            });
            cx.new(|cx| gpui_component::Root::new(view, window, cx).bg(cx.theme().background))
        },
    ) {
        Ok(handle) => {
            let handle: AnyWindowHandle = handle.into();
            let keep_window = {
                let mut session = quick_add_session.borrow_mut();
                let keep_window = session.request_serial == request_serial
                    && session.presentation == Some(QuickAddPresentation::Window);
                if keep_window {
                    session.handle = Some(handle);
                }
                keep_window
            };

            if !keep_window {
                let _ = handle.update(cx, |_, window, _| {
                    window.remove_window();
                });
                return;
            }

            // When the window is opened from a title-bar click, macOS can briefly
            // return focus to the main window after the click completes.
            // Re-activating the quick add window on the next turn keeps it visible.
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| {
                    window.activate_window();
                });
            });
        }
        Err(err) => {
            let should_clear = {
                let session = quick_add_session.borrow();
                session.request_serial == request_serial
                    && session.presentation == Some(QuickAddPresentation::Window)
            };
            if should_clear {
                quick_add_session.borrow_mut().clear();
            }
            eprintln!("[QuickAdd] Failed to open window: {}", err);
        }
    }
}

fn close_visible_quick_add(
    cx: &mut App,
    controller: &Rc<RefCell<MainWindowController>>,
    quick_add_session: &Rc<RefCell<QuickAddSessionController>>,
) {
    let (presentation, handle, hide_app_on_close) = {
        let session = quick_add_session.borrow();
        (
            session.presentation,
            session.handle,
            session.hide_app_on_close,
        )
    };

    match presentation {
        Some(QuickAddPresentation::Window) => {
            if let Some(handle) = handle {
                let _ = handle.update(cx, |_, window, _| {
                    window.remove_window();
                });
            }
        }
        Some(QuickAddPresentation::Overlay) => {
            let _ = dismiss_quick_add_overlay(cx, controller);
        }
        None => {}
    }

    let should_hide = {
        let mut session = quick_add_session.borrow_mut();
        let should_hide = should_hide_app_after_quick_add_close(
            hide_app_on_close && !session.has_draft(),
            live_window_handles(cx).len(),
        );
        session.clear();
        should_hide
    };

    if should_hide {
        cx.hide();
    }
}

fn show_quick_add_hotkey_protection(
    cx: &mut App,
    controller: &Rc<RefCell<MainWindowController>>,
    quick_add_session: &Rc<RefCell<QuickAddSessionController>>,
) {
    let presentation = quick_add_session.borrow().presentation;
    match presentation {
        Some(QuickAddPresentation::Window) => {
            let handle = quick_add_session.borrow().handle;
            if let Some(handle) = handle {
                let _ = handle.update(cx, |root_view, window, cx| {
                    if let Ok(root) = root_view.downcast::<gpui_component::Root>() {
                        root.update(cx, |root, cx| {
                            if let Ok(view) = root.view().clone().downcast::<QuickAddWindow>() {
                                view.update(cx, |view, cx| view.show_hotkey_protection(cx));
                            }
                        });
                    }
                    window.activate_window();
                });
            }
        }
        Some(QuickAddPresentation::Overlay) => {
            let _ = show_overlay_hotkey_protection(cx, controller);
        }
        None => {}
    }
}

fn live_window_handles(cx: &App) -> Vec<AnyWindowHandle> {
    cx.window_stack().unwrap_or_else(|| cx.windows())
}

fn main_window_exists(cx: &mut App, controller: &Rc<RefCell<MainWindowController>>) -> bool {
    sync_main_window_controller(cx, controller);
    if controller.borrow().handle.is_some() {
        return true;
    }

    live_window_handles(cx)
        .into_iter()
        .any(|handle| is_main_window_handle(cx, handle))
}

fn sync_main_window_controller(cx: &mut App, controller: &Rc<RefCell<MainWindowController>>) {
    let tracked_window_id = controller.borrow().window_id;
    let Some(tracked_window_id) = tracked_window_id else {
        return;
    };

    if let Some(handle) = live_window_handles(cx)
        .into_iter()
        .find(|handle| handle.window_id() == tracked_window_id)
    {
        controller.borrow_mut().handle = Some(handle);
        return;
    }

    controller.borrow_mut().clear_handle();
}

fn is_main_window_handle(cx: &mut App, handle: AnyWindowHandle) -> bool {
    handle
        .update(cx, |root_view, _window, cx| {
            if let Ok(root) = root_view.downcast::<gpui_component::Root>() {
                root.update(cx, |root, _cx| {
                    root.view().clone().downcast::<MainView>().is_ok()
                })
            } else {
                false
            }
        })
        .unwrap_or(false)
}

fn resolve_main_window_handle(
    cx: &mut App,
    controller: &Rc<RefCell<MainWindowController>>,
) -> Option<AnyWindowHandle> {
    sync_main_window_controller(cx, controller);

    if let Some(handle) = controller.borrow().handle {
        return Some(handle);
    }

    let current_panel = controller.borrow().current_panel;
    for handle in live_window_handles(cx) {
        if is_main_window_handle(cx, handle) {
            controller.borrow_mut().track(handle, current_panel);
            return Some(handle);
        }
    }

    None
}

fn show_quick_add_overlay(
    cx: &mut App,
    controller: &Rc<RefCell<MainWindowController>>,
    preferred_mode: Option<InputMode>,
) -> bool {
    let Some(handle) = resolve_main_window_handle(cx, controller) else {
        return false;
    };

    handle
        .update(cx, |root_view, window, cx| {
            if let Ok(root) = root_view.downcast::<gpui_component::Root>() {
                root.update(cx, |root, cx| {
                    if let Ok(main_view) = root.view().clone().downcast::<MainView>() {
                        main_view.update(cx, |this, cx| {
                            this.show_quick_add_overlay(preferred_mode, window, cx);
                        });
                    }
                });
            }
            window.activate_window();
        })
        .is_ok()
}

fn focus_quick_add_overlay(cx: &mut App, controller: &Rc<RefCell<MainWindowController>>) -> bool {
    let Some(handle) = resolve_main_window_handle(cx, controller) else {
        return false;
    };

    handle
        .update(cx, |root_view, window, cx| {
            if let Ok(root) = root_view.downcast::<gpui_component::Root>() {
                root.update(cx, |root, cx| {
                    if let Ok(main_view) = root.view().clone().downcast::<MainView>() {
                        main_view.update(cx, |this, cx| this.focus_quick_add_overlay(window, cx));
                    }
                });
            }
            window.activate_window();
        })
        .is_ok()
}

fn dismiss_quick_add_overlay(cx: &mut App, controller: &Rc<RefCell<MainWindowController>>) -> bool {
    let Some(handle) = resolve_main_window_handle(cx, controller) else {
        return false;
    };

    handle
        .update(cx, |root_view, _window, cx| {
            if let Ok(root) = root_view.downcast::<gpui_component::Root>() {
                root.update(cx, |root, cx| {
                    if let Ok(main_view) = root.view().clone().downcast::<MainView>() {
                        main_view.update(cx, |this, cx| this.dismiss_quick_add_overlay(cx));
                    }
                });
            }
        })
        .is_ok()
}

fn show_overlay_hotkey_protection(
    cx: &mut App,
    controller: &Rc<RefCell<MainWindowController>>,
) -> bool {
    let Some(handle) = resolve_main_window_handle(cx, controller) else {
        return false;
    };

    handle
        .update(cx, |root_view, window, cx| {
            if let Ok(root) = root_view.downcast::<gpui_component::Root>() {
                root.update(cx, |root, cx| {
                    if let Ok(main_view) = root.view().clone().downcast::<MainView>() {
                        main_view.update(cx, |this, cx| {
                            this.show_quick_add_hotkey_protection(window, cx);
                        });
                    }
                });
            }
            window.activate_window();
        })
        .is_ok()
}

fn switch_existing_main_window(
    cx: &mut App,
    controller: &Rc<RefCell<MainWindowController>>,
    panel: Panel,
) -> bool {
    let Some(handle) = resolve_main_window_handle(cx, controller) else {
        return false;
    };

    if handle
        .update(cx, |root_view, window, cx| {
            update_main_window(root_view, window, cx, panel)
        })
        .ok()
        == Some(true)
    {
        controller.borrow_mut().track(handle, panel);
        return true;
    }

    controller.borrow_mut().clear_handle();
    false
}

fn ensure_main_window(
    cx: &mut App,
    controller: &Rc<RefCell<MainWindowController>>,
    store: &Store,
    quick_add_session: &Rc<RefCell<QuickAddSessionController>>,
    target_panel: Option<Panel>,
) {
    let desired_panel = target_panel.unwrap_or_else(|| controller.borrow().current_panel);

    if switch_existing_main_window(cx, controller, desired_panel) {
        return;
    }

    match open_main_window(
        cx,
        store.clone(),
        controller.clone(),
        quick_add_session.clone(),
        desired_panel,
    ) {
        Ok(handle) => {
            controller.borrow_mut().track(handle, desired_panel);
        }
        Err(err) => {
            eprintln!("[MainWindow] Failed to open main window: {}", err);
        }
    }
}

fn update_main_window(root_view: AnyView, window: &mut Window, cx: &mut App, panel: Panel) -> bool {
    let mut updated = false;
    if let Ok(root) = root_view.downcast::<gpui_component::Root>() {
        root.update(cx, |root, cx| {
            if let Ok(main_view) = root.view().clone().downcast::<MainView>() {
                updated = true;
                main_view.update(cx, |this, cx| this.switch_to_panel(panel, window, cx));
            }
        });
    }
    if updated {
        window.activate_window();
    }
    updated
}

fn open_main_window(
    cx: &mut App,
    store: Store,
    controller: Rc<RefCell<MainWindowController>>,
    quick_add_session: Rc<RefCell<QuickAddSessionController>>,
    initial_panel: Panel,
) -> Result<AnyWindowHandle> {
    let window_size = size(px(900.0), px(600.0));
    let window_bounds = WindowBounds::Windowed(Bounds::centered(None, window_size, cx));

    cx.open_window(
        WindowOptions {
            window_bounds: Some(window_bounds),
            titlebar: Some(TitleBar::title_bar_options()),
            ..Default::default()
        },
        move |window, cx| {
            let store = store.clone();
            let controller = controller.clone();
            let quick_add_session = quick_add_session.clone();
            let view = cx.new(|cx| {
                MainView::new(
                    store,
                    controller,
                    quick_add_session,
                    initial_panel,
                    window,
                    cx,
                )
            });
            cx.new(|cx| gpui_component::Root::new(view, window, cx).bg(cx.theme().background))
        },
    )
    .map(|h| h.into())
}

pub struct MainView {
    store: Store,
    current_panel: Panel,
    current_settings_section: SettingsSection,
    dashboard_panel: Entity<Dashboard>,
    data_management_panel: Entity<DataManagementPanel>,
    search_panel: Entity<SearchPanel>,
    task_panel: Entity<TaskPanel>,
    timeline_panel: Entity<Timeline>,
    notes_panel: Entity<NotePanel>,
    quick_add_overlay: Option<Entity<QuickAddWindow>>,
    shortcut_config: ShortcutConfig,
    window_state: Rc<RefCell<MainWindowController>>,
    quick_add_session: Rc<RefCell<QuickAddSessionController>>,
    focus_handle: FocusHandle,
    _window_activation_subscription: Subscription,
}

impl MainView {
    fn new(
        store: Store,
        window_state: Rc<RefCell<MainWindowController>>,
        quick_add_session: Rc<RefCell<QuickAddSessionController>>,
        initial_panel: Panel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let store_for_panels = store.clone();
        let dashboard_panel = cx.new(|cx| Dashboard::new(store_for_panels.clone(), window, cx));
        let data_management_panel =
            cx.new(|cx| DataManagementPanel::new(store_for_panels.clone(), window, cx));
        let search_panel = cx.new(|cx| SearchPanel::new(store_for_panels.clone(), window, cx));
        let task_panel = cx.new(|cx| TaskPanel::new(store_for_panels.clone(), window, cx));
        let timeline_panel = cx.new(|cx| Timeline::new(store_for_panels.clone(), window, cx));
        let notes_panel = cx.new(|cx| NotePanel::new(store_for_panels, window, cx));
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        window_state.borrow_mut().current_panel = initial_panel;
        let window_state_for_activation = window_state.clone();
        let window_activation_subscription =
            cx.observe_window_activation(window, move |_this, window, _cx| {
                let mut controller = window_state_for_activation.borrow_mut();
                controller.is_active = window.is_window_active();
            });

        let handle = cx.entity().clone();
        dashboard_panel.update(cx, |panel, _cx| {
            panel.on_action(move |action, window, cx| {
                handle.update(cx, |this, cx| {
                    this.handle_dashboard_action(action, window, cx);
                });
            });
        });

        Self {
            store,
            current_panel: initial_panel,
            current_settings_section: SettingsSection::DataManagement,
            dashboard_panel,
            data_management_panel,
            search_panel,
            task_panel,
            timeline_panel,
            notes_panel,
            quick_add_overlay: None,
            shortcut_config: ShortcutConfig::load(),
            window_state,
            quick_add_session,
            focus_handle,
            _window_activation_subscription: window_activation_subscription,
        }
    }

    fn handle_dashboard_action(
        &mut self,
        action: DashboardAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            DashboardAction::OpenTaskPreset(preset) => {
                self.switch_to_panel(Panel::Tasks, window, cx);
                self.task_panel.update(cx, |panel, cx| {
                    panel.apply_focus_preset(preset, window, cx);
                });
            }
            DashboardAction::OpenTimeline => {
                self.switch_to_panel(Panel::Timeline, window, cx);
            }
            DashboardAction::FilterByTag(tag) => {
                self.switch_to_panel(Panel::Timeline, window, cx);
                self.timeline_panel.update(cx, |panel, cx| {
                    panel.apply_filters(vec![tag], Vec::new(), cx);
                });
            }
            DashboardAction::FilterByPerson(person) => {
                self.switch_to_panel(Panel::Timeline, window, cx);
                self.timeline_panel.update(cx, |panel, cx| {
                    panel.apply_filters(Vec::new(), vec![person], cx);
                });
            }
        }
    }

    fn focus_active_panel(&mut self, panel: Panel, window: &mut Window, cx: &mut Context<Self>) {
        if self.quick_add_overlay.is_some() {
            self.focus_quick_add_overlay(window, cx);
            return;
        }

        match panel {
            Panel::Search => {
                self.search_panel.update(cx, |panel, cx| {
                    panel.focus_input(window, cx);
                });
            }
            _ => {
                self.focus_handle.focus(window, cx);
            }
        }
    }

    fn quick_add_open_destination(&self) -> Arc<dyn Fn(QuickAddDestination, &mut App)> {
        let store = self.store.clone();
        let main_window = self.window_state.clone();
        let quick_add_session = self.quick_add_session.clone();
        Arc::new(move |destination, cx| match destination {
            QuickAddDestination::Main => {
                ensure_main_window(cx, &main_window, &store, &quick_add_session, None);
            }
            QuickAddDestination::Tasks => {
                ensure_main_window(
                    cx,
                    &main_window,
                    &store,
                    &quick_add_session,
                    Some(Panel::Tasks),
                );
            }
            QuickAddDestination::Records => {
                ensure_main_window(
                    cx,
                    &main_window,
                    &store,
                    &quick_add_session,
                    Some(Panel::Records),
                );
            }
        })
    }

    fn show_quick_add_overlay(
        &mut self,
        preferred_mode: Option<InputMode>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(overlay) = self.quick_add_overlay.clone() {
            overlay.update(cx, |overlay, cx| {
                overlay.focus_input(window, cx);
            });
            let mut session = self.quick_add_session.borrow_mut();
            session.mark_visible(QuickAddPresentation::Overlay);
            session.handle = None;
            session.hide_app_on_close = false;
            window.activate_window();
            return;
        }

        {
            let mut session = self.quick_add_session.borrow_mut();
            if let Some(mode) = preferred_mode {
                session.mode = mode;
            }
            session.mark_visible(QuickAddPresentation::Overlay);
            session.handle = None;
            session.hide_app_on_close = false;
        }

        let dismiss_overlay: Arc<dyn Fn(&mut App)> = {
            let main_view = cx.entity().clone();
            Arc::new(move |cx| {
                let _ = main_view.update(cx, |this, cx| {
                    this.dismiss_quick_add_overlay(cx);
                });
            })
        };

        let store = self.store.clone();
        let session = self.quick_add_session.clone();
        let open_destination = self.quick_add_open_destination();
        let overlay = cx.new(|cx| {
            let mut view = QuickAddWindow::new_overlay(
                store,
                session,
                open_destination,
                dismiss_overlay,
                window,
                cx,
            );
            view.hide_app_on_close = false;
            view
        });
        self.quick_add_overlay = Some(overlay);
        cx.notify();
    }

    fn focus_quick_add_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(overlay) = self.quick_add_overlay.clone() {
            overlay.update(cx, |overlay, cx| {
                overlay.focus_input(window, cx);
            });
            let mut session = self.quick_add_session.borrow_mut();
            session.mark_visible(QuickAddPresentation::Overlay);
            session.handle = None;
            session.hide_app_on_close = false;
            window.activate_window();
        }
    }

    fn dismiss_quick_add_overlay(&mut self, cx: &mut Context<Self>) {
        self.quick_add_overlay = None;
        cx.notify();
    }

    fn show_quick_add_hotkey_protection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(overlay) = self.quick_add_overlay.clone() {
            overlay.update(cx, |overlay, cx| {
                overlay.show_hotkey_protection(cx);
                overlay.focus_input(window, cx);
            });
        }
    }

    fn open_quick_add_from_titlebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let store = self.store.clone();
        let main_window = self.window_state.clone();
        let quick_add_session = self.quick_add_session.clone();

        window.activate_window();
        cx.defer(move |cx| {
            open_or_focus_quick_add_overlay(cx, store, main_window, quick_add_session);
        });
    }

    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        TitleBar::new()
            .child(
                h_flex().h_full().items_center().child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0x595959))
                        .child(format!("Robinne · {}", self.current_panel.title())),
                ),
            )
            .child(
                h_flex().h_full().items_center().pr(px(12.0)).child(
                    Button::new("main-titlebar-quick-add")
                        .ghost()
                        .small()
                        .icon(IconName::Plus)
                        .on_click(cx.listener(|this, _event, window, cx| {
                            this.open_quick_add_from_titlebar(window, cx);
                        })),
                ),
            )
    }

    pub fn switch_to_panel(&mut self, panel: Panel, window: &mut Window, cx: &mut Context<Self>) {
        if panel == Panel::Settings && self.current_panel != Panel::Settings {
            self.current_settings_section = SettingsSection::DataManagement;
            self.data_management_panel
                .update(cx, |panel, cx| panel.refresh(cx));
        }

        if self.current_panel != panel {
            eprintln!(
                "[MainView] Switching panel from {:?} to {:?}",
                self.current_panel, panel
            );
            self.current_panel = panel;
            self.window_state.borrow_mut().current_panel = panel;
            cx.notify();
        }

        self.focus_active_panel(panel, window, cx);
    }

    fn settings_content_width(window: &Window) -> Pixels {
        let viewport_width = window.viewport_size().width;
        let sidebar_width = main_sidebar_width(main_sidebar_layout_mode(viewport_width));
        std::cmp::max(viewport_width - sidebar_width, px(0.0))
    }

    fn current_settings_layout_mode(&self, window: &Window) -> SettingsLayoutMode {
        if Self::settings_content_width(window) >= SETTINGS_NAV_BREAKPOINT {
            SettingsLayoutMode::Sidebar
        } else {
            SettingsLayoutMode::TopTabs
        }
    }

    fn render_settings_nav_item(
        &self,
        section: SettingsSection,
        is_active: bool,
        layout_mode: SettingsLayoutMode,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let px_x = match layout_mode {
            SettingsLayoutMode::Sidebar => px(12.0),
            SettingsLayoutMode::TopTabs => px(14.0),
        };
        let py_y = match layout_mode {
            SettingsLayoutMode::Sidebar => px(10.0),
            SettingsLayoutMode::TopTabs => px(8.0),
        };

        div()
            .id(("settings-section", section as usize))
            .cursor_pointer()
            .px(px_x)
            .py(py_y)
            .rounded(px(10.0))
            .bg(if is_active {
                rgb(0xf5f5f0)
            } else {
                rgb(0xffffff)
            })
            .hover(|style| {
                style.bg(if is_active {
                    rgb(0xf5f5f0)
                } else {
                    rgb(0xf7f7f7)
                })
            })
            .child(
                div()
                    .text_sm()
                    .font_weight(if is_active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(if is_active {
                        rgb(0x262626)
                    } else {
                        rgb(0x595959)
                    })
                    .child(section.label()),
            )
            .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                this.current_settings_section = section;
                if section == SettingsSection::DataManagement {
                    this.data_management_panel
                        .update(cx, |panel, cx| panel.refresh(cx));
                }
                cx.notify();
            }))
    }

    fn render_settings_placeholder(&self, section: SettingsSection) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0x262626))
                    .child(section.title()),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x8c8c8c))
                    .line_height(relative(1.5))
                    .child(section.description()),
            )
            .child(
                div()
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(px(999.0))
                    .bg(rgb(0xf5f5f5))
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0x8c8c8c))
                    .child("开发中"),
            )
    }

    fn render_shortcuts_settings(&self) -> impl IntoElement {
        let global_shortcut_entries = self
            .shortcut_config
            .entries()
            .into_iter()
            .map(|(label, shortcut)| (label.to_string(), shortcut.to_string()))
            .collect::<Vec<_>>();
        let app_shortcut_entries = app_shortcut_entries()
            .into_iter()
            .map(|(label, shortcut)| (label.to_string(), shortcut.to_string()))
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0x262626))
                    .child("快捷键"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x8c8c8c))
                    .line_height(relative(1.5))
                    .child(app_shortcuts_intro()),
            )
            .child(self.render_shortcut_group(
                "应用内快捷键",
                app_shortcut_scope_description(),
                app_shortcut_entries,
            ))
            .child(self.render_shortcut_group(
                "全局快捷键",
                global_shortcut_scope_description(),
                global_shortcut_entries,
            ))
    }

    fn render_shortcut_group(
        &self,
        title: &'static str,
        description: &'static str,
        shortcut_entries: Vec<(String, String)>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0x262626))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x8c8c8c))
                            .line_height(relative(1.5))
                            .child(description),
                    ),
            )
            .child(div().flex().flex_col().gap(px(10.0)).children(
                shortcut_entries.into_iter().map(|(label, shortcut)| {
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(px(14.0))
                        .py(px(12.0))
                        .rounded(px(12.0))
                        .border_1()
                        .border_color(rgb(0xf0f0f0))
                        .bg(rgb(0xfafafa))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(0x262626))
                                .child(label),
                        )
                        .child(
                            div()
                                .text_sm()
                                .font_family(".SystemUIFont")
                                .text_color(rgb(0x0958d9))
                                .child(shortcut),
                        )
                }),
            ))
    }

    fn render_settings_content(&self) -> impl IntoElement {
        match self.current_settings_section {
            SettingsSection::DataManagement => {
                self.data_management_panel.clone().into_any_element()
            }
            SettingsSection::General => self
                .render_settings_placeholder(self.current_settings_section)
                .into_any_element(),
            SettingsSection::Shortcuts => self.render_shortcuts_settings().into_any_element(),
            SettingsSection::About => self
                .render_settings_placeholder(self.current_settings_section)
                .into_any_element(),
        }
    }

    fn render_settings_panel(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout_mode = self.current_settings_layout_mode(window);
        let sections = SettingsSection::all();
        let current_section = self.current_settings_section;

        div()
            .size_full()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0x262626))
                    .child("设置"),
            )
            .child(match layout_mode {
                SettingsLayoutMode::Sidebar => div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap(px(24.0))
                    .child(
                        div()
                            .w(SETTINGS_SIDEBAR_NAV_WIDTH)
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .children(sections.into_iter().map(|section| {
                                self.render_settings_nav_item(
                                    section,
                                    current_section == section,
                                    layout_mode,
                                    cx,
                                )
                                .into_any_element()
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .min_h_0()
                            .overflow_hidden()
                            .child(
                                div()
                                    .size_full()
                                    .pr(px(16.0))
                                    .overflow_y_scrollbar()
                                    .child(self.render_settings_content()),
                            ),
                    )
                    .into_any_element(),
                SettingsLayoutMode::TopTabs => div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(div().flex().gap(px(8.0)).children(sections.into_iter().map(
                        |section| {
                            self.render_settings_nav_item(
                                section,
                                current_section == section,
                                layout_mode,
                                cx,
                            )
                            .into_any_element()
                        },
                    )))
                    .child(
                        div().flex_1().min_h_0().overflow_hidden().child(
                            div()
                                .size_full()
                                .pr(px(16.0))
                                .overflow_y_scrollbar()
                                .child(self.render_settings_content()),
                        ),
                    )
                    .into_any_element(),
            })
    }
}

impl Focusable for MainView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_panel = self.current_panel;
        let sidebar_layout_mode = main_sidebar_layout_mode(window.viewport_size().width);
        let on_panel_change = cx.listener(
            |this: &mut MainView,
             panel: &Panel,
             window: &mut Window,
             cx: &mut Context<MainView>| {
                this.switch_to_panel(*panel, window, cx);
            },
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xf0f0f0))
            .track_focus(&self.focus_handle(cx))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if !event.keystroke.modifiers.platform {
                    return;
                }

                if event.keystroke.key == "0" {
                    window.activate_window();
                    return;
                }

                if let Some((_, panel)) = main_panel_shortcuts()
                    .into_iter()
                    .find(|(key, _)| *key == event.keystroke.key.as_str())
                {
                    this.switch_to_panel(panel, window, cx);
                }
            }))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .overflow_hidden()
                    .flex_col()
                    .child(self.render_title_bar(cx))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .overflow_hidden()
                            .relative()
                            .child(
                                Sidebar::new(move |panel, window, app| {
                                    on_panel_change(&panel, window, app);
                                })
                                .with_panel(current_panel)
                                .with_layout_mode(sidebar_layout_mode),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .overflow_hidden()
                                    .relative()
                                    .bg(rgb(0xffffff))
                                    .child(match self.current_panel {
                                        Panel::Dashboard => {
                                            self.dashboard_panel.clone().into_any_element()
                                        }
                                        Panel::Tasks => self.task_panel.clone().into_any_element(),
                                        Panel::Records => {
                                            self.notes_panel.clone().into_any_element()
                                        }
                                        Panel::Timeline => {
                                            self.timeline_panel.clone().into_any_element()
                                        }
                                        Panel::Search => {
                                            self.search_panel.clone().into_any_element()
                                        }
                                        Panel::AI => div()
                                            .size_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(8.0))
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .text_xl()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(rgb(0x262626))
                                                            .child("AI 面板开发中..."),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .text_color(rgb(0x8c8c8c))
                                                            .child("当前版本先保留占位态，后续再补充完整交互。"),
                                                    ),
                                            )
                                            .into_any_element(),
                                        Panel::Settings => {
                                            self.render_settings_panel(window, cx).into_any_element()
                                        }
                                    }),
                            )
                            .when_some(self.quick_add_overlay.clone(), |el, overlay| {
                                el.child(overlay)
                            }),
                    ),
            )
    }
}
