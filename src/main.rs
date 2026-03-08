use beitang::store::{create_store, Store};
use beitang::ui::sidebar::{Panel, Sidebar};
use beitang::ui::task_panel::TaskPanel;
use beitang::shortcut_manager::{ShortcutManager, ShortcutEvent};
use beitang::ui::floating_window::QuickAddWindow;
use gpui::*;
use gpui_component::{ActiveTheme, Root};
use gpui_platform::application;
use std::sync::Arc;
use std::sync::Mutex;

fn main() {
    let app = application();

    app.run(move |cx| {
        // 初始化 gpui-component
        gpui_component::init(cx);

        // 强制使用浅色主题
        gpui_component::Theme::change(gpui_component::ThemeMode::Light, None, cx);

        // 创建异步 store
        let (store, mut runtime) = create_store();
        let store_for_shortcuts = store.clone();

        // 后台运行 store
        cx.spawn(|_cx: &mut AsyncApp| async move {
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("beitang");
            std::fs::create_dir_all(&data_dir).ok();

            let db_path = data_dir.join("data.db");
            runtime.run(db_path).await;
        }).detach();

        // 初始化快捷键管理器
        let (_shortcut_manager, event_rx) = ShortcutManager::new()
            .expect("Failed to create shortcut manager");

        // 创建通道用于桥接同步快捷键事件到 GPUI 异步上下文
        let (shortcut_tx, mut shortcut_rx) = tokio::sync::mpsc::channel::<ShortcutEvent>(100);

        // 存储主窗口句柄，用于后续激活
        let main_window_handle: Arc<Mutex<Option<WindowHandle<Root>>>> = Arc::new(Mutex::new(None));
        let main_window_handle_for_shortcuts = main_window_handle.clone();

        // 异步打开窗口
        cx.spawn(async move |cx| {
            let window_handle = cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|cx| MainView::new(store, window, cx));
                cx.new(|cx| {
                    gpui_component::Root::new(view, window, cx)
                        .bg(cx.theme().background)
                })
            })?;

            // 保存窗口句柄
            *main_window_handle_for_shortcuts.lock().unwrap() = Some(window_handle);

            Ok::<_, anyhow::Error>(())
        }).detach();

        // 启动线程监听快捷键事件，并通过通道发送
        std::thread::spawn(move || {
            while let Ok(event) = event_rx.recv() {
                eprintln!("[Shortcut] Received event: {:?}", event);
                if shortcut_tx.blocking_send(event).is_err() {
                    eprintln!("[Shortcut] Failed to send event to channel");
                    break;
                }
            }
        });

        // 在 GPUI 异步上下文中监听快捷键事件
        let main_window_handle_for_listener = main_window_handle.clone();
        cx.spawn(async move |cx| {
            while let Some(event) = shortcut_rx.recv().await {
                match event {
                    ShortcutEvent::QuickAddTask => {
                        // 打开快速添加任务窗口
                        let store = store_for_shortcuts.clone();
                        let _ = cx.update(|cx| {
                            cx.open_window(
                                WindowOptions {
                                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                                        None,
                                        size(px(400.0), px(80.0)),
                                        cx,
                                    ))),
                                    ..Default::default()
                                },
                                |window, cx| {
                                    cx.new(|cx| QuickAddWindow::new(store, window, cx))
                                },
                            )
                        });
                    }
                    ShortcutEvent::QuickAddNote => {
                        // 快速添加笔记（占位符）
                        eprintln!("[Shortcut] QuickAddNote not yet implemented");
                    }
                    ShortcutEvent::ViewTasks => {
                        // 激活主窗口并切换到任务面板
                        let handle = main_window_handle_for_listener.clone();
                        let _ = cx.update(|cx| {
                            if let Some(window_handle) = handle.lock().unwrap().as_ref() {
                                window_handle.update(cx, |_root, _window, _cx| {
                                    // Root 包含 MainView，我们需要访问它
                                    // 由于 Root 是 gpui_component 的类型，我们需要通过其他方式访问
                                    // 暂时只激活窗口
                                    eprintln!("[Shortcut] ViewTasks - window activated");
                                }).ok();
                            }
                        });
                    }
                    ShortcutEvent::ViewNotes => {
                        // 激活主窗口并切换到笔记面板（占位符）
                        eprintln!("[Shortcut] ViewNotes not yet implemented");
                    }
                    ShortcutEvent::OpenMain => {
                        // 激活主窗口
                        let handle = main_window_handle_for_listener.clone();
                        let _ = cx.update(|cx| {
                            if let Some(window_handle) = handle.lock().unwrap().as_ref() {
                                window_handle.update(cx, |_root, _window, _cx| {
                                    // 窗口已激活，无需额外操作
                                    eprintln!("[Shortcut] OpenMain - window activated");
                                }).ok();
                            }
                        });
                    }
                }
            }
        }).detach();
    });
}

pub struct MainView {
    current_panel: Panel,
    task_panel: Entity<TaskPanel>,
}

impl MainView {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let task_panel = cx.new(|cx| TaskPanel::new(store, window, cx));
        Self {
            current_panel: Panel::Tasks,
            task_panel,
        }
    }

    pub fn switch_to_panel(&mut self, panel: Panel, _window: &mut Window, cx: &mut Context<Self>) {
        eprintln!("[MainView] Switching panel from {:?} to {:?}", self.current_panel, panel);
        self.current_panel = panel;
        cx.notify();
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

        div()
            .size_full()
            .flex()
            .bg(rgb(0xf0f0f0))
            .child(Sidebar::new(move |panel, _window, _cx| {
                on_panel_change(&panel, _window, _cx)
            }).with_panel(current_panel))
            .child(
                div()
                    .flex_1()
                    .p(px(24.0))
                    .bg(rgb(0xffffff))  // 白色背景便于看清
                    .child(match self.current_panel {
                        Panel::Tasks => self.task_panel.clone().into_any_element(),
                        _ => div().child(format!("{:?} Panel", self.current_panel)).into_any_element(),
                    })
            )
    }
}
