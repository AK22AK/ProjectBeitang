mod models;
mod db;
mod store;

use gpui::*;
use store::{create_store, Store};

fn main() {
    App::new().run(|cx: &mut AppContext| {
        // Create async store
        let (store, runtime) = create_store();

        // Spawn store runtime in background
        cx.spawn(|_| async move {
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("beitang");
            std::fs::create_dir_all(&data_dir).ok();

            let db_path = data_dir.join("data.db");
            runtime.run(db_path).await.ok();
        }).detach();

        // Compute window bounds first to avoid borrow issues
        let window_bounds = WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1200.0), px(800.0)),
            cx,
        ));

        // Open main window
        cx.open_window(
            WindowOptions {
                window_bounds: Some(window_bounds),
                ..Default::default()
            },
            |cx| {
                cx.new_view(|cx| MainView::new(store, cx))
            },
        ).unwrap();
    });
}

pub struct MainView {
    store: Store,
}

impl MainView {
    pub fn new(store: Store, _cx: &mut ViewContext<Self>) -> Self {
        Self { store }
    }
}

impl Render for MainView {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x1a1a1a))
            .child("Hello, Beitang!")
    }
}
