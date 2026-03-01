mod models;
mod db;
mod store;
mod ui;

use gpui::*;
use gpui_platform::application;
use store::{create_store, Store};
use ui::sidebar::{Panel, Sidebar};
use ui::task_panel::TaskPanel;

fn main() {
    let app = application().with_assets(gpui_component_assets::Assets);

    app.run(|cx: &mut App| {
        // Initialize gpui-component (REQUIRED before using any components)
        gpui_component::init(cx);

        // Create async store
        let (store, runtime) = create_store();

        // Spawn store runtime in background
        cx.spawn(|_cx: &mut AsyncApp| async move {
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("beitang");
            std::fs::create_dir_all(&data_dir).ok();

            let db_path = data_dir.join("data.db");
            runtime.run(db_path).await.ok();
        }).detach();

        // Calculate window bounds in sync context
        let window_bounds = Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1200.0), px(800.0)),
            cx,
        )));

        // Open main window with Root wrapper (required by gpui-component)
        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds,
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| MainView::new(store, window, cx));
                    cx.new(|cx| gpui_component::Root::new(view, window, cx))
                },
            )?;
            Ok::<_, anyhow::Error>(())
        }).detach();
    });
}

pub struct MainView {
    store: Store,
    current_panel: Panel,
    task_panel: Entity<TaskPanel>,
}

impl MainView {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let task_panel = cx.new(|cx| TaskPanel::new(store.clone(), window, cx));
        Self {
            store,
            current_panel: Panel::Tasks,
            task_panel,
        }
    }
}

impl Render for MainView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_panel = self.current_panel;
        let on_panel_change = cx.listener(|this: &mut MainView, panel: &Panel, _window: &mut Window, _cx: &mut Context<MainView>| {
            this.current_panel = *panel;
        });

        div()
            .size_full()
            .flex()
            .bg(rgb(0xf0f0f0))
            .text_color(rgb(0x000000))
            .child(Sidebar::new(move |panel, _window, _cx| on_panel_change(&panel, _window, _cx)).with_panel(current_panel))
            .child(
                div()
                    .flex_1()
                    .p(px(24.0))
                    .child(match self.current_panel {
                        Panel::Tasks => self.task_panel.clone().into_any_element(),
                        _ => div().child(format!("{:?} Panel", self.current_panel)).into_any_element(),
                    })
            )
    }
}
