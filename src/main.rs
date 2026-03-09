use beitang::store::{create_store, Store};
use beitang::ui::sidebar::{Panel, Sidebar};
use beitang::ui::task_panel::TaskPanel;
use beitang::ui::note_panel::NotePanel;
use beitang::ui::floating_window::QuickAddWindow;
use gpui::*;
use gpui_component::ActiveTheme;
use gpui_platform::application;

fn main() {
    let app = application();

    app.run(move |cx| {
        // 初始化 gpui-component
        gpui_component::init(cx);

        // 强制使用浅色主题
        gpui_component::Theme::change(gpui_component::ThemeMode::Light, None, cx);

        // 创建异步 store
        let (store, mut runtime) = create_store();

        // 后台运行 store
        cx.spawn(|_cx: &mut AsyncApp| async move {
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("beitang");
            std::fs::create_dir_all(&data_dir).ok();

            let db_path = data_dir.join("data.db");
            runtime.run(db_path).await;
        }).detach();

        // 异步打开窗口
        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|cx| MainView::new(store, window, cx));
                cx.new(|cx| {
                    gpui_component::Root::new(view, window, cx)
                        .bg(cx.theme().background)
                })
            })?;
            Ok::<_, anyhow::Error>(())
        }).detach();
    });
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

        // 每次渲染时确保主视图有焦点，以便捕获键盘事件
        self.focus_handle(cx).focus(_window, cx);

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
                                    cx.new(|cx| QuickAddWindow::new(store, window, cx))
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
                                    cx.new(|cx| QuickAddWindow::new_for_note(store, window, cx))
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
