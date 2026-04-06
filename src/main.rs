use beitang::config::ShortcutConfig;
use beitang::store::{create_store, Store};
use beitang::ui::dashboard::Dashboard;
use beitang::ui::floating_window::{
    quick_add_window_size, QuickAddDestination, QuickAddSessionController, QuickAddSessionStatus,
    QuickAddWindow,
};
use beitang::ui::note_panel::NotePanel;
use beitang::ui::search::SearchPanel;
use beitang::ui::sidebar::{Panel, Sidebar};
use beitang::ui::task_panel::TaskPanel;
use beitang::ui::timeline::Timeline;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use gpui_component::ActiveTheme;
use gpui_component_assets::Assets;
use gpui_platform::application;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

const MAIN_SIDEBAR_WIDTH: Pixels = px(200.0);
const SETTINGS_NAV_BREAKPOINT: Pixels = px(600.0);
const SETTINGS_SIDEBAR_NAV_WIDTH: Pixels = px(180.0);

#[derive(Clone)]
struct MainWindowController {
    handle: Option<AnyWindowHandle>,
    current_panel: Panel,
}

impl Default for MainWindowController {
    fn default() -> Self {
        Self {
            handle: None,
            current_panel: Panel::Dashboard,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsSection {
    General,
    Shortcuts,
    About,
}

impl SettingsSection {
    fn label(self) -> &'static str {
        match self {
            Self::General => "通用",
            Self::Shortcuts => "快捷键",
            Self::About => "关于",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::General => "通用",
            Self::Shortcuts => "快捷键",
            Self::About => "关于",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::General => "应用级偏好、显示方式等通用设置将在这里集中管理。",
            Self::Shortcuts => "当前先展示全局快捷键，后续再开放自定义编辑。",
            Self::About => "版本信息、更新说明和相关说明将在这里统一展示。",
        }
    }

    fn is_implemented(self) -> bool {
        matches!(self, Self::Shortcuts)
    }

    fn all() -> [Self; 3] {
        [Self::General, Self::Shortcuts, Self::About]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsLayoutMode {
    Sidebar,
    TopTabs,
}

fn main() {
    let app = application().with_assets(Assets);

    let (store, mut runtime) = create_store();
    let shortcuts = ShortcutConfig::load();

    let main_window = Rc::new(RefCell::new(MainWindowController::default()));
    let quick_add_session = Rc::new(RefCell::new(QuickAddSessionController::default()));

    let main_window_for_reopen = main_window.clone();
    let store_for_reopen = store.clone();
    app.on_reopen(move |cx| {
        activate_main_window(cx, &main_window_for_reopen, &store_for_reopen, None);
    });

    let main_window_for_run = main_window.clone();
    let quick_add_for_run = quick_add_session.clone();
    let store_for_run = store.clone();
    app.run(move |cx| {
        gpui_component::init(cx);
        gpui_component::Theme::change(gpui_component::ThemeMode::Light, None, cx);

        cx.spawn(|_cx: &mut AsyncApp| async move {
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("beitang");
            std::fs::create_dir_all(&data_dir).ok();

            let db_path = data_dir.join("data.db");
            runtime.run(db_path).await;
        })
        .detach();

        activate_main_window(
            cx,
            &main_window_for_run,
            &store_for_run,
            Some(Panel::Dashboard),
        );

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
                                                activate_main_window(
                                                    cx,
                                                    &main_window_for_hotkey,
                                                    &store_for_hotkey,
                                                    None,
                                                );
                                            } else if event.id == open_tasks.id() {
                                                activate_main_window(
                                                    cx,
                                                    &main_window_for_hotkey,
                                                    &store_for_hotkey,
                                                    Some(Panel::Tasks),
                                                );
                                            } else if event.id == open_records.id() {
                                                activate_main_window(
                                                    cx,
                                                    &main_window_for_hotkey,
                                                    &store_for_hotkey,
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
    let status = quick_add_session.borrow().status;
    match status {
        QuickAddSessionStatus::Closed => {
            let hide_app_on_close = cx.active_window().is_none();
            open_quick_add_window(cx, store, main_window, quick_add_session, hide_app_on_close);
        }
        QuickAddSessionStatus::Dormant => {
            open_quick_add_window(cx, store, main_window, quick_add_session, false);
        }
        QuickAddSessionStatus::Visible => {
            if quick_add_session.borrow().has_draft() {
                show_quick_add_hotkey_protection(cx, &quick_add_session);
            } else {
                close_visible_quick_add(cx, &quick_add_session);
            }
        }
    }
}

fn open_quick_add_window(
    cx: &mut App,
    store: Store,
    main_window: Rc<RefCell<MainWindowController>>,
    quick_add_session: Rc<RefCell<QuickAddSessionController>>,
    hide_app_on_close: bool,
) {
    {
        let mut session = quick_add_session.borrow_mut();
        session.status = QuickAddSessionStatus::Visible;
        session.handle = None;
        session.hide_app_on_close = hide_app_on_close;
    }

    cx.activate(true);

    let window_size = quick_add_window_size();
    let window_bounds = WindowBounds::Windowed(Bounds::centered(None, window_size, cx));

    let open_destination: Arc<dyn Fn(QuickAddDestination, &mut App)> = {
        let store = store.clone();
        let main_window = main_window.clone();
        Arc::new(move |destination, cx| match destination {
            QuickAddDestination::Main => {
                activate_main_window(cx, &main_window, &store, None);
            }
            QuickAddDestination::Tasks => {
                activate_main_window(cx, &main_window, &store, Some(Panel::Tasks));
            }
            QuickAddDestination::Records => {
                activate_main_window(cx, &main_window, &store, Some(Panel::Records));
            }
        })
    };

    let session_for_window = quick_add_session.clone();
    let open_destination_for_window = open_destination.clone();
    let store_for_window = store.clone();

    match cx.open_window(
        WindowOptions {
            window_bounds: Some(window_bounds),
            ..Default::default()
        },
        move |window, cx| {
            let session = session_for_window.clone();
            let open_destination = open_destination_for_window.clone();
            let store = store_for_window.clone();
            let view = cx.new(|cx| {
                let mut view = QuickAddWindow::new(store, session, open_destination, window, cx);
                view.hide_app_on_close = hide_app_on_close;
                view
            });
            cx.new(|cx| gpui_component::Root::new(view, window, cx).bg(cx.theme().background))
        },
    ) {
        Ok(handle) => {
            quick_add_session.borrow_mut().handle = Some(handle.into());
        }
        Err(err) => {
            quick_add_session.borrow_mut().clear();
            eprintln!("[QuickAdd] Failed to open window: {}", err);
        }
    }
}

fn close_visible_quick_add(
    cx: &mut App,
    quick_add_session: &Rc<RefCell<QuickAddSessionController>>,
) {
    let (handle, hide_app_on_close) = {
        let session = quick_add_session.borrow();
        (session.handle, session.hide_app_on_close)
    };

    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window, _| {
            window.remove_window();
        });
    }

    let should_hide = {
        let mut session = quick_add_session.borrow_mut();
        let should_hide = hide_app_on_close && !session.has_draft();
        session.clear();
        should_hide
    };

    if should_hide {
        cx.hide();
    }
}

fn show_quick_add_hotkey_protection(
    cx: &mut App,
    quick_add_session: &Rc<RefCell<QuickAddSessionController>>,
) {
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

fn activate_main_window(
    cx: &mut App,
    controller: &Rc<RefCell<MainWindowController>>,
    store: &Store,
    target_panel: Option<Panel>,
) {
    let desired_panel = target_panel.unwrap_or_else(|| controller.borrow().current_panel);
    let existing_handle = controller.borrow().handle;

    if let Some(handle) = existing_handle {
        if handle
            .update(cx, |root_view, window, cx| {
                update_main_window(root_view, window, cx, desired_panel);
            })
            .is_ok()
        {
            controller.borrow_mut().current_panel = desired_panel;
            return;
        }
    }

    match open_main_window(cx, store.clone(), controller.clone(), desired_panel) {
        Ok(handle) => {
            let mut state = controller.borrow_mut();
            state.handle = Some(handle);
            state.current_panel = desired_panel;
        }
        Err(err) => {
            eprintln!("[MainWindow] Failed to open main window: {}", err);
        }
    }
}

fn update_main_window(root_view: AnyView, window: &mut Window, cx: &mut App, panel: Panel) {
    if let Ok(root) = root_view.downcast::<gpui_component::Root>() {
        root.update(cx, |root, cx| {
            if let Ok(main_view) = root.view().clone().downcast::<MainView>() {
                main_view.update(cx, |this, cx| this.switch_to_panel(panel, window, cx));
            }
        });
    }
    window.activate_window();
}

fn open_main_window(
    cx: &mut App,
    store: Store,
    controller: Rc<RefCell<MainWindowController>>,
    initial_panel: Panel,
) -> Result<AnyWindowHandle> {
    let window_size = size(px(900.0), px(600.0));
    let window_bounds = WindowBounds::Windowed(Bounds::centered(None, window_size, cx));

    cx.open_window(
        WindowOptions {
            window_bounds: Some(window_bounds),
            ..Default::default()
        },
        move |window, cx| {
            let store = store.clone();
            let controller = controller.clone();
            let view = cx.new(|cx| MainView::new(store, controller, initial_panel, window, cx));
            cx.new(|cx| gpui_component::Root::new(view, window, cx).bg(cx.theme().background))
        },
    )
    .map(|h| h.into())
}

pub struct MainView {
    current_panel: Panel,
    current_settings_section: SettingsSection,
    dashboard_panel: Entity<Dashboard>,
    search_panel: Entity<SearchPanel>,
    task_panel: Entity<TaskPanel>,
    timeline_panel: Entity<Timeline>,
    notes_panel: Entity<NotePanel>,
    shortcut_config: ShortcutConfig,
    window_state: Rc<RefCell<MainWindowController>>,
    focus_handle: FocusHandle,
}

impl MainView {
    fn new(
        store: Store,
        window_state: Rc<RefCell<MainWindowController>>,
        initial_panel: Panel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let store_for_panels = store.clone();
        let dashboard_panel = cx.new(|cx| Dashboard::new(store_for_panels.clone(), window, cx));
        let search_panel = cx.new(|cx| SearchPanel::new(store_for_panels.clone(), window, cx));
        let task_panel = cx.new(|cx| TaskPanel::new(store_for_panels.clone(), window, cx));
        let timeline_panel = cx.new(|cx| Timeline::new(store_for_panels.clone(), window, cx));
        let notes_panel = cx.new(|cx| NotePanel::new(store_for_panels, window, cx));
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        window_state.borrow_mut().current_panel = initial_panel;

        Self {
            current_panel: initial_panel,
            current_settings_section: SettingsSection::General,
            dashboard_panel,
            search_panel,
            task_panel,
            timeline_panel,
            notes_panel,
            shortcut_config: ShortcutConfig::load(),
            window_state,
            focus_handle,
        }
    }

    fn focus_active_panel(&mut self, panel: Panel, window: &mut Window, cx: &mut Context<Self>) {
        match panel {
            Panel::Search => {
                self.search_panel.update(cx, |panel, cx| {
                    panel.focus_input(window, cx);
                });
            }
            Panel::Tasks => {
                self.task_panel.update(cx, |panel, cx| {
                    panel.focus_primary_input(window, cx);
                });
            }
            Panel::Records => {
                self.notes_panel.update(cx, |panel, cx| {
                    panel.focus_primary_input(window, cx);
                });
            }
            _ => {
                self.focus_handle.focus(window, cx);
            }
        }
    }

    pub fn switch_to_panel(&mut self, panel: Panel, window: &mut Window, cx: &mut Context<Self>) {
        if panel == Panel::Settings && self.current_panel != Panel::Settings {
            self.current_settings_section = SettingsSection::General;
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
        std::cmp::max(window.viewport_size().width - MAIN_SIDEBAR_WIDTH, px(0.0))
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
        let shortcut_entries = self
            .shortcut_config
            .entries()
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
                    .child("查看当前可用的全局快捷键。后续版本会在这里补充自定义编辑能力。"),
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
        if self.current_settings_section.is_implemented() {
            self.render_shortcuts_settings().into_any_element()
        } else {
            self.render_settings_placeholder(self.current_settings_section)
                .into_any_element()
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
            .bg(rgb(0xf0f0f0))
            .track_focus(&self.focus_handle(cx))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if !event.keystroke.modifiers.platform {
                    return;
                }

                match event.keystroke.key.as_str() {
                    "0" => window.activate_window(),
                    "1" => this.switch_to_panel(Panel::Dashboard, window, cx),
                    "2" => this.switch_to_panel(Panel::Tasks, window, cx),
                    "3" => this.switch_to_panel(Panel::Records, window, cx),
                    "4" => this.switch_to_panel(Panel::Timeline, window, cx),
                    "5" => this.switch_to_panel(Panel::Search, window, cx),
                    _ => {}
                }
            }))
            .child(
                Sidebar::new(move |panel, window, app| {
                    on_panel_change(&panel, window, app);
                })
                .with_panel(current_panel),
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
                        Panel::Dashboard => self.dashboard_panel.clone().into_any_element(),
                        Panel::Tasks => self.task_panel.clone().into_any_element(),
                        Panel::Records => self.notes_panel.clone().into_any_element(),
                        Panel::Timeline => self.timeline_panel.clone().into_any_element(),
                        Panel::Search => self.search_panel.clone().into_any_element(),
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
    }
}
