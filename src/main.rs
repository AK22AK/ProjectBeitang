use beitang::store::{create_store, Store};
use beitang::ui::sidebar::{Panel, Sidebar};
use beitang::ui::task_panel::TaskPanel;
use beitang::ui::note_panel::NotePanel;
use beitang::ui::floating_window::QuickAddWindow;
use global_hotkey::{GlobalHotKeyManager, HotKeyState, hotkey::{HotKey, Modifiers, Code}, GlobalHotKeyEvent};
use gpui::*;
use gpui_component::ActiveTheme;
use gpui_platform::application;

fn main() {
    let app = application();

    // 创建异步 store
    let (store, mut runtime) = create_store();
    
    let store_for_main = store.clone();
    let store_for_reopen = store.clone();
    let store_for_hotkey = store.clone();

    let mut main_window_for_reopen: Option<AnyWindowHandle> = None;
    
    app.on_reopen(move |cx| {
        let mut needs_open = true;
        if let Some(handle) = main_window_for_reopen.as_ref() {
            if handle.update(cx, |_, window, _| {
                window.activate_window();
            }).is_ok() {
                needs_open = false;
            }
        }
        
        if needs_open {
            main_window_for_reopen = open_main_window(cx, store_for_reopen.clone()).ok();
        }
    });

    app.run(move |cx| {
        // 初始化 gpui-component
        gpui_component::init(cx);

        // 强制使用浅色主题
        gpui_component::Theme::change(gpui_component::ThemeMode::Light, None, cx);

        // 后台运行 store
        cx.spawn(|_cx: &mut AsyncApp| async move {
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("beitang");
            std::fs::create_dir_all(&data_dir).ok();

            let db_path = data_dir.join("data.db");
            runtime.run(db_path).await;
        }).detach();

        // 异步打开原始的主窗口
        let store_local = store_for_main.clone();
        let _main_handle = open_main_window(cx, store_local).ok();
        // 这里只是为了初始显示，真正的持久句柄由 on_reopen 闭包持有（如果能共用更好，但由于 cx 借用限制，先让 initial 打开）
        // 如果想让 initial 窗口也能被 Dock 激活，需要更复杂的同步。先解决 Dock 点击能开窗的问题。

        // --- 全局快捷键注册 ---
        // 注意：要在能够维持生命周期的作用域内保存 manager 防止其被 drop 而注销快捷键
        if let Ok(manager) = GlobalHotKeyManager::new() {
            let hotkey = HotKey::new(Some(Modifiers::META | Modifiers::SHIFT), Code::KeyT); // Cmd+Shift+T
            if let Err(e) = manager.register(hotkey) {
                eprintln!("[Global Hotkey] Failed to register: {}", e);
            } else {
                eprintln!("[Global Hotkey] Registered Cmd+Shift+T");
                
                let store_for_hotkey = store_for_hotkey.clone();
                cx.spawn(async move |cx| {
                    // 持有 manager，保证热键生效
                    let _manager = manager;
                    let receiver = GlobalHotKeyEvent::receiver();
                    let mut quick_add_window: Option<gpui::AnyWindowHandle> = None;
                    
                    loop {
                        if let Ok(event) = receiver.try_recv() {
                            if event.id == hotkey.id() && event.state == HotKeyState::Released {
                                // 使用 update 跳到了由 App context 支持的主线程环境
                                cx.update(|cx| {
                                    let was_active = cx.active_window().is_some();
                                    
                                    // 唤醒当前应用使其成为屏幕前置焦点
                                    cx.activate(true);
                                    
                                    let mut closed_existing = false;
                                    if let Some(handle) = quick_add_window.take() {
                                        if handle.update(cx, |_, window, _| {
                                            window.remove_window();
                                        }).is_ok() {
                                            closed_existing = true;
                                            // 如果是重新 toggle 关掉，并且应用原本不在前台，一并隐藏整个应用
                                            if !was_active {
                                                cx.hide();
                                            }
                                        }
                                    }
                                    
                                    if !closed_existing {
                                        let store = store_for_hotkey.clone();
                                        
                                        // 预置一个小窗大小
                                        let window_size = size(px(400.0), px(80.0));
                                        let window_bounds = WindowBounds::Windowed(Bounds::centered(None, window_size, cx));
                                        
                                        if let Ok(handle) = cx.open_window(
                                            WindowOptions {
                                                window_bounds: Some(window_bounds),
                                                ..Default::default()
                                            },
                                            |window, cx| {
                                                let view = cx.new(|cx| {
                                                    let mut view = QuickAddWindow::new(store, window, cx);
                                                    view.hide_app_on_close = !was_active;
                                                    view
                                                });
                                                cx.new(|cx| {
                                                    gpui_component::Root::new(view, window, cx)
                                                        .bg(cx.theme().background)
                                                })
                                            },
                                        ) {
                                            quick_add_window = Some(handle.into());
                                        }
                                    }
                                });
                            }
                        }
                        
                        // 定期检查（这里 GPUI 的 async timer 最好基于 background_executor）
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(100))
                            .await;
                    }
                }).detach();
            }
        } else {
            eprintln!("[Global Hotkey] Initialization failed!");
        }
    });
}

fn open_main_window(cx: &mut App, store: Store) -> Result<AnyWindowHandle> {
    cx.open_window(WindowOptions::default(), |window, cx| {
        let view = cx.new(|cx| MainView::new(store, window, cx));
        cx.new(|cx| {
            gpui_component::Root::new(view, window, cx)
                .bg(cx.theme().background)
        })
    }).map(|h| h.into())
}

pub struct MainView {
    current_panel: Panel,
    task_panel: Entity<TaskPanel>,
    note_panel: Entity<NotePanel>,
    store: Store,
    focus_handle: FocusHandle,
}

impl MainView {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store_for_panels = store.clone();
        let task_panel = cx.new(|cx| TaskPanel::new(store_for_panels.clone(), window, cx));
        let note_panel = cx.new(|cx| NotePanel::new(store_for_panels, window, cx));
        let focus_handle = cx.focus_handle();
        // 初始时请求焦点，使键盘事件可以被捕获
        focus_handle.focus(window, cx);
        Self {
            current_panel: Panel::Tasks,
            task_panel,
            note_panel,
            store,
            focus_handle,
        }
    }

    pub fn switch_to_panel(&mut self, panel: Panel, cx: &mut Context<Self>) {
        eprintln!("[MainView] Switching panel from {:?} to {:?}", self.current_panel, panel);
        self.current_panel = panel;
        cx.notify();
    }
}

impl Focusable for MainView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MainView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_panel = self.current_panel;
        let on_panel_change = cx.listener(|this: &mut MainView, panel: &Panel, _window: &mut Window, cx: &mut Context<MainView>| {
            eprintln!("Panel changing from {:?} to {:?}", this.current_panel, panel);
            this.current_panel = *panel;
            cx.notify();  // 强制刷新界面
        });

        // 获取 store 用于快捷键打开浮动窗口
        let store_for_shortcut = self.store.clone();

        div()
            .size_full()
            .flex()
            .bg(rgb(0xf0f0f0))
            .track_focus(&self.focus_handle(cx))
            // 添加 GPUI 层面的键盘快捷键处理
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                let modifiers = event.keystroke.modifiers;
                let key = event.keystroke.key.as_str();

                // 检查是否是 Cmd 键 (macOS 的 platform 修饰符)
                let is_cmd = modifiers.platform;

                if is_cmd {
                    // 预计算窗口大小以避免借用冲突
                    let window_size = size(px(400.0), px(80.0));
                    match key {
                        "n" => {
                            eprintln!("[MainView] Cmd+N pressed - opening quick add task");
                            let store = store_for_shortcut.clone();
                            let window_bounds = WindowBounds::Windowed(Bounds::centered(None, window_size, cx));
                            cx.open_window(
                                WindowOptions {
                                    window_bounds: Some(window_bounds),
                                    ..Default::default()
                                },
                                |window, cx| {
                                    let view = cx.new(|cx| QuickAddWindow::new(store, window, cx));
                                    cx.new(|cx| {
                                        gpui_component::Root::new(view, window, cx)
                                            .bg(cx.theme().background)
                                    })
                                },
                            ).ok();
                        }
                        "m" => {
                            eprintln!("[MainView] Cmd+M pressed - opening quick add note");
                            let store = store_for_shortcut.clone();
                            let window_bounds = WindowBounds::Windowed(Bounds::centered(None, window_size, cx));
                            cx.open_window(
                                WindowOptions {
                                    window_bounds: Some(window_bounds),
                                    ..Default::default()
                                },
                                |window, cx| {
                                    let view = cx.new(|cx| QuickAddWindow::new_for_note(store, window, cx));
                                    cx.new(|cx| {
                                        gpui_component::Root::new(view, window, cx)
                                            .bg(cx.theme().background)
                                    })
                                },
                            ).ok();
                        }
                        "1" => {
                            eprintln!("[MainView] Cmd+1 pressed - switching to Tasks");
                            this.switch_to_panel(Panel::Tasks, cx);
                        }
                        "2" => {
                            eprintln!("[MainView] Cmd+2 pressed - switching to Notes");
                            this.switch_to_panel(Panel::Notes, cx);
                        }
                        "0" => {
                            eprintln!("[MainView] Cmd+0 pressed - activating window");
                            window.activate_window();
                        }
                        _ => {}
                    }
                }
            }))
            .child(Sidebar::new(move |panel, _window, _cx| {
                on_panel_change(&panel, _window, _cx)
            }).with_panel(current_panel))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .p(px(24.0))
                    .bg(rgb(0xffffff))  // 白色背景便于看清
                    .child(match self.current_panel {
                        Panel::Tasks => self.task_panel.clone().into_any_element(),
                        Panel::Notes => self.note_panel.clone().into_any_element(),
                        _ => div().child(format!("{:?} Panel", self.current_panel)).into_any_element(),
                    })
            )
    }
}
