use chrono::{DateTime, Utc};
use gpui::{prelude::*, *};
use gpui_component::{
    button::Button,
    h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement,
    v_flex,
};

use crate::models::Record;

pub struct RecordDetailSidebar {
    current_record_id: Option<String>,
    record_title: Option<String>,
    record_content: String,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
    tags: Vec<String>,
    persons: Vec<String>,
    title_input: Option<Entity<InputState>>,
    content_input: Option<Entity<InputState>>,
    on_save: Option<Box<dyn Fn(SavePayload, &mut Context<Self>) + Send + Sync>>,
    on_close: Option<Box<dyn Fn(&mut Context<Self>) + Send + Sync>>,
}

/// 保存时的数据载荷
#[derive(Debug, Clone)]
pub struct SavePayload {
    pub record_id: String,
    pub title: Option<String>,
    pub content: String,
}

/// 侧边栏显示状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SidebarState {
    Hidden,
    Visible,
}

impl RecordDetailSidebar {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            current_record_id: None,
            record_title: None,
            record_content: String::new(),
            created_at: None,
            updated_at: None,
            tags: Vec::new(),
            persons: Vec::new(),
            title_input: None,
            content_input: None,
            on_save: None,
            on_close: None,
        }
    }

    pub fn on_save<F>(&mut self, callback: F)
    where
        F: Fn(SavePayload, &mut Context<Self>) + Send + Sync + 'static,
    {
        self.on_save = Some(Box::new(callback));
    }

    pub fn on_close<F>(&mut self, callback: F)
    where
        F: Fn(&mut Context<Self>) + Send + Sync + 'static,
    {
        self.on_close = Some(Box::new(callback));
    }

    /// 显示记录详情 - 只在 record_id 变化时才重建 UI 状态
    pub fn show_record(&mut self, record: &Record, window: &mut Window, cx: &mut Context<Self>) {
        let record_id = record.id.to_string();

        // 关键：如果已经在显示同一个记录，什么都不做
        if self.current_record_id.as_ref() == Some(&record_id) {
            return;
        }

        // 更新记录数据
        self.current_record_id = Some(record_id);
        self.record_title = record.title.clone();
        self.record_content = record.content.clone();
        self.created_at = Some(record.created_at);
        self.updated_at = Some(record.updated_at);
        self.tags = record.tags.clone();
        self.persons = record.persons.clone();

        // 初始化或更新标题输入框
        let title_value = record.title.clone().unwrap_or_default();
        if let Some(ref input) = self.title_input {
            input.update(cx, |state, cx| {
                state.set_value(&title_value, window, cx);
            });
        } else {
            let title_input = cx.new(|cx| {
                let mut input = InputState::new(window, cx);
                input.set_value(&title_value, window, cx);
                input
            });
            self.title_input = Some(title_input);
        }

        // 初始化或更新内容输入框
        if let Some(ref input) = self.content_input {
            input.update(cx, |state, cx| {
                state.set_value(&record.content, window, cx);
            });
        } else {
            let content_input = cx.new(|cx| {
                let mut input = InputState::new(window, cx);
                input.set_value(&record.content, window, cx);
                input
            });
            self.content_input = Some(content_input);
        }

        cx.notify();
    }

    /// 关闭侧边栏
    pub fn close(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.current_record_id = None;
        cx.notify();
    }

    /// 获取当前状态
    pub fn state(&self) -> SidebarState {
        if self.current_record_id.is_some() {
            SidebarState::Visible
        } else {
            SidebarState::Hidden
        }
    }

    /// 获取当前记录 ID
    pub fn current_record_id(&self) -> Option<&str> {
        self.current_record_id.as_deref()
    }

    fn save_changes(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ref record_id) = self.current_record_id {
            let title = self
                .title_input
                .as_ref()
                .map(|input| {
                    let val = input.read(cx).value().to_string();
                    if val.trim().is_empty() {
                        None
                    } else {
                        Some(val)
                    }
                })
                .unwrap_or_else(|| self.record_title.clone());

            let content = self
                .content_input
                .as_ref()
                .map(|input| input.read(cx).value().to_string())
                .unwrap_or_else(|| self.record_content.clone());

            let payload = SavePayload {
                record_id: record_id.clone(),
                title,
                content,
            };

            if let Some(ref callback) = self.on_save {
                callback(payload, cx);
            }
        }
    }
}

impl Render for RecordDetailSidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_visible = self.current_record_id.is_some();
        if !is_visible {
            return div().into_any_element();
        }

        let content_input_clone = self.content_input.clone();

        div()
            .id("record-detail-sidebar")
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .flex_row()
            .justify_end()
            .cursor_default()
            .child(
                div()
                    .id("record-detail-sidebar-dismiss-area")
                    .flex_1()
                    .h_full()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|_this, _event, _window, cx| {
                            cx.stop_propagation();
                        }),
                    )
                    .on_click(cx.listener(|this, _event: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.close(window, cx);
                        if let Some(ref callback) = this.on_close {
                            callback(cx);
                        }
                    })),
            )
            .child(
                div()
                    .id("record-detail-sidebar-pane")
                    .w(px(360.0))
                    .h_full()
                    .flex()
                    .flex_col()
                    .occlude()
                    .overflow_hidden()
                    .border_l_1()
                    .border_color(rgb(0xe8e8e8))
                    .bg(rgb(0xffffff))
                    .cursor_default()
                    .on_click(cx.listener(|_this, _event: &ClickEvent, _window, cx| {
                        cx.stop_propagation();
                    }))
                    .child(
                        div()
                            .p(px(12.0))
                            .border_b_1()
                            .border_color(rgb(0xe8e8e8))
                            .cursor_default()
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child("记录详情"),
                                    )
                                    .child(
                                        Button::new("sidebar-close-detail").child("✕").on_click(
                                            cx.listener(|this, _event, window, cx| {
                                                this.close(window, cx);
                                                if let Some(ref callback) = this.on_close {
                                                    callback(cx);
                                                }
                                            }),
                                        ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .cursor_default()
                            .child(
                                v_flex()
                                    .p(px(12.0))
                                    .gap(px(12.0))
                                    .overflow_y_scrollbar()
                                    // 标题输入
                                    .child(
                                        v_flex()
                                            .gap(px(6.0))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x666666))
                                                    .child("标题"),
                                            )
                                            .when_some(self.title_input.clone(), |el, input| {
                                                el.child(
                                                    div()
                                                        .on_mouse_down(
                                                            gpui::MouseButton::Left,
                                                            cx.listener(
                                                                |_this, _event, _window, cx| {
                                                                    cx.stop_propagation();
                                                                },
                                                            ),
                                                        )
                                                        .child(
                                                            Input::new(&input)
                                                                .appearance(false)
                                                                .text_size(px(16.0))
                                                                .font_weight(
                                                                    gpui::FontWeight::SEMIBOLD,
                                                                ),
                                                        ),
                                                )
                                            }),
                                    )
                                    // 内容输入
                                    .child(
                                        v_flex()
                                            .gap(px(6.0))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x666666))
                                                    .child("内容"),
                                            )
                                            .when_some(content_input_clone.clone(), |el, input| {
                                                el.child(
                                                    div()
                                                        .on_mouse_down(
                                                            gpui::MouseButton::Left,
                                                            cx.listener(
                                                                |_this, _event, _window, cx| {
                                                                    cx.stop_propagation();
                                                                },
                                                            ),
                                                        )
                                                        .child(
                                                            Input::new(&input)
                                                                .appearance(false)
                                                                .text_size(px(14.0)),
                                                        ),
                                                )
                                            }),
                                    )
                                    // 创建时间
                                    .child(
                                        v_flex()
                                            .gap(px(6.0))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x666666))
                                                    .child("创建时间"),
                                            )
                                            .child(
                                                div().text_sm().text_color(rgb(0x999999)).child(
                                                    self.created_at
                                                        .map(|dt| {
                                                            dt.with_timezone(&chrono::Local)
                                                                .format("%Y-%m-%d %H:%M")
                                                                .to_string()
                                                        })
                                                        .unwrap_or_else(|| "-".to_string()),
                                                ),
                                            ),
                                    )
                                    // 更新时间
                                    .child(
                                        v_flex()
                                            .gap(px(6.0))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x666666))
                                                    .child("更新时间"),
                                            )
                                            .child(
                                                div().text_sm().text_color(rgb(0x999999)).child(
                                                    self.updated_at
                                                        .map(|dt| {
                                                            dt.with_timezone(&chrono::Local)
                                                                .format("%Y-%m-%d %H:%M")
                                                                .to_string()
                                                        })
                                                        .unwrap_or_else(|| "-".to_string()),
                                                ),
                                            ),
                                    )
                                    // 标签
                                    .when(!self.tags.is_empty(), |el| {
                                        el.child(
                                            v_flex()
                                                .gap(px(6.0))
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(rgb(0x666666))
                                                        .child("标签"),
                                                )
                                                .child(h_flex().gap(px(6.0)).flex_wrap().children(
                                                    self.tags.iter().enumerate().map(
                                                        |(idx, tag)| {
                                                            div()
                                                                .id(("record-sidebar-tag", idx))
                                                                .px(px(8.0))
                                                                .py(px(4.0))
                                                                .rounded(px(12.0))
                                                                .bg(rgb(0xf5f5f5))
                                                                .text_sm()
                                                                .text_color(rgb(0x595959))
                                                                .child(format!("#{}", tag))
                                                        },
                                                    ),
                                                )),
                                        )
                                    })
                                    // 相关人物
                                    .when(!self.persons.is_empty(), |el| {
                                        el.child(
                                            v_flex()
                                                .gap(px(6.0))
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(rgb(0x666666))
                                                        .child("相关人物"),
                                                )
                                                .child(h_flex().gap(px(6.0)).flex_wrap().children(
                                                    self.persons.iter().enumerate().map(
                                                        |(idx, person)| {
                                                            div()
                                                                .id(("record-sidebar-person", idx))
                                                                .px(px(8.0))
                                                                .py(px(4.0))
                                                                .rounded(px(12.0))
                                                                .bg(rgb(0xe6f7ff))
                                                                .text_sm()
                                                                .text_color(rgb(0x1890ff))
                                                                .child(format!("@{}", person))
                                                        },
                                                    ),
                                                )),
                                        )
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .p(px(12.0))
                            .border_t_1()
                            .border_color(rgb(0xe8e8e8))
                            .cursor_default()
                            .child(
                                Button::new("sidebar-save-detail")
                                    .w_full()
                                    .child("保存修改")
                                    .on_click(cx.listener(|this, _event, window, cx| {
                                        this.save_changes(window, cx);
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
}
