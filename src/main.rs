mod models;
mod db;
mod store;
mod ui;

use gpui::*;
use gpui_component::ActiveTheme;
use gpui_platform::application;
use store::{create_store, Store};
use ui::sidebar::{Panel, Sidebar};
use ui::task_panel::TaskPanel;

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
}

impl MainView {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let task_panel = cx.new(|cx| TaskPanel::new(store, window, cx));
        Self {
            current_panel: Panel::Tasks,
            task_panel,
        }
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
