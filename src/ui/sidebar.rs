use gpui::*;
use gpui::prelude::*;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Panel {
    Tasks,
    Records,
    Timeline,
    Ai,
}

#[derive(IntoElement)]
pub struct Sidebar {
    current_panel: Panel,
    on_panel_select: Arc<dyn Fn(Panel, &mut Window, &mut App)>,
}

impl Sidebar {
    pub fn new<F>(on_select: F) -> Self
    where
        F: Fn(Panel, &mut Window, &mut App) + 'static,
    {
        Self {
            current_panel: Panel::Tasks,
            on_panel_select: Arc::new(on_select),
        }
    }

    pub fn with_panel(mut self, panel: Panel) -> Self {
        self.current_panel = panel;
        self
    }
}

impl RenderOnce for Sidebar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let items = vec![
            (Panel::Tasks, "任务"),
            (Panel::Records, "记录"),
            (Panel::Timeline, "时间线"),
            (Panel::Ai, "AI"),
        ];

        div()
            .w(px(200.0))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .bg(rgb(0x333333))
            .text_color(rgb(0xffffff))
            .p(px(12.0))
            .children(items.into_iter().enumerate().map(move |(idx, (panel, label))| {
                let is_active = self.current_panel == panel;
                let on_click = self.on_panel_select.clone();

                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .when(is_active, |this| this.bg(rgb(0x4a4a4a)))
                    .hover(|style| style.bg(rgb(0x444444)))
                    .flex()
                    .gap(px(8.0))
                    .items_center()
                    .child(label)
                    .id(idx)
                    .on_click(move |_event, window, cx| on_click(panel, window, cx))
            }))
    }
}
