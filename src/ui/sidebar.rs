use gpui::prelude::*;
use gpui::*;
use gpui_component::IconName;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Panel {
    Dashboard,
    Tasks,
    Records,
    Timeline,
    AI,
    Search,
    Settings,
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
        let current_panel = self.current_panel;
        let on_panel_select = self.on_panel_select.clone();

        let main_items: Vec<(Panel, &'static str, IconName)> = vec![
            (Panel::Dashboard, "看板", IconName::GalleryVerticalEnd),
            (Panel::Tasks, "任务", IconName::Check),
            (Panel::Records, "记录", IconName::File),
            (Panel::Timeline, "时间线", IconName::Calendar),
            (Panel::AI, "AI", IconName::Bot),
        ];

        let bottom_items: Vec<(Panel, &'static str, IconName)> = vec![
            (Panel::Search, "搜索", IconName::Search),
            (Panel::Settings, "设置", IconName::Settings),
        ];

        let on_panel_select_for_main = on_panel_select.clone();

        div()
            .w(px(200.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(0xf5f5f5))
            .text_color(rgb(0x333333))
            .p(px(12.0))
            .child(
                div().flex().flex_col().gap(px(4.0)).flex_1().children(
                    main_items
                        .into_iter()
                        .enumerate()
                        .map(move |(idx, (panel, label, icon))| {
                            let is_active = current_panel == panel;
                            let on_click = on_panel_select_for_main.clone();

                            div()
                                .px(px(12.0))
                                .py(px(8.0))
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .when(is_active, |this| this.bg(rgb(0xd0d0d0)))
                                .hover(|style| style.bg(rgb(0xe0e0e0)))
                                .flex()
                                .gap(px(8.0))
                                .items_center()
                                .child(
                                    gpui_component::Icon::new(icon)
                                        .size(px(18.0))
                                        .text_color(rgb(0x555555)),
                                )
                                .child(label)
                                .id(idx)
                                .on_click(move |_event, window, cx| on_click(panel, window, cx))
                        }),
                ),
            )
            .child(
                div().flex().flex_col().gap(px(4.0)).pt(px(16.0)).children(
                    bottom_items
                        .into_iter()
                        .enumerate()
                        .map(move |(idx, (panel, label, icon))| {
                            let is_active = current_panel == panel;
                            let on_click = on_panel_select.clone();

                            div()
                                .px(px(12.0))
                                .py(px(8.0))
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .when(is_active, |this| this.bg(rgb(0xd0d0d0)))
                                .hover(|style| style.bg(rgb(0xe0e0e0)))
                                .flex()
                                .gap(px(8.0))
                                .items_center()
                                .child(
                                    gpui_component::Icon::new(icon)
                                        .size(px(18.0))
                                        .text_color(rgb(0x555555)),
                                )
                                .child(label)
                                .id(idx + 100)
                                .on_click(move |_event, window, cx| on_click(panel, window, cx))
                        }),
                ),
            )
    }
}
