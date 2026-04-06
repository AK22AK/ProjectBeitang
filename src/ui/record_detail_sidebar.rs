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
use crate::ui::parsing;

pub struct RecordDetailSidebar {
    current_record_id: Option<String>,
    record_title: Option<String>,
    record_content: String,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
    tags: Vec<String>,
    persons: Vec<String>,
    inline_tags: Vec<String>,
    inline_persons: Vec<String>,
    title_input: Option<Entity<InputState>>,
    content_input: Option<Entity<InputState>>,
    content_expanded: bool,
    on_save: Option<Box<dyn Fn(SavePayload, &mut Context<Self>) + Send + Sync>>,
    on_delete: Option<Box<dyn Fn(String, &mut Context<Self>) + Send + Sync>>,
    on_close: Option<Box<dyn Fn(&mut Context<Self>) + Send + Sync>>,
}

/// 保存时的数据载荷
#[derive(Debug, Clone)]
pub struct SavePayload {
    pub record_id: String,
    pub title: Option<String>,
    pub content: String,
    pub tags: Vec<String>,
    pub persons: Vec<String>,
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
            inline_tags: Vec::new(),
            inline_persons: Vec::new(),
            title_input: None,
            content_input: None,
            content_expanded: false,
            on_save: None,
            on_delete: None,
            on_close: None,
        }
    }

    pub fn on_save<F>(&mut self, callback: F)
    where
        F: Fn(SavePayload, &mut Context<Self>) + Send + Sync + 'static,
    {
        self.on_save = Some(Box::new(callback));
    }

    pub fn on_delete<F>(&mut self, callback: F)
    where
        F: Fn(String, &mut Context<Self>) + Send + Sync + 'static,
    {
        self.on_delete = Some(Box::new(callback));
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
        let inline_fields = parsing::parse_record_fields(record.title.as_deref(), &record.content);
        self.inline_tags = inline_fields.tags;
        self.inline_persons = inline_fields.people;

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

        // 初始化或更新内容输入框（多行文本区域）
        let content_value = record.content.clone();
        if let Some(ref input) = self.content_input {
            input.update(cx, |state, cx| {
                state.set_value(&content_value, window, cx);
            });
        } else {
            let content_input = cx.new(|cx| {
                let mut input = InputState::new(window, cx).multi_line(true).auto_grow(1, 6);
                input.set_value(&content_value, window, cx);
                input
            });
            self.content_input = Some(content_input);
        }

        cx.notify();
    }

    /// 关闭侧边栏
    pub fn close(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss(cx);
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
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

    /// 切换内容输入框的展开/收起状态
    fn toggle_content_expanded(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.content_expanded = !self.content_expanded;
        cx.notify();
    }

    const APPROX_CHARS_PER_LINE: usize = 45;

    fn estimate_line_count(content: &str) -> usize {
        if content.is_empty() {
            return 1;
        }

        let newline_count = content.matches('\n').count();

        // Estimate additional lines based on character width
        // Chinese characters count as 2 units, ASCII characters count as 1 unit
        let total_width: usize = content
            .chars()
            .map(|c| if c.is_ascii() { 1 } else { 2 })
            .sum();

        let estimated_lines_from_width =
            (total_width + Self::APPROX_CHARS_PER_LINE - 1) / Self::APPROX_CHARS_PER_LINE;

        let estimated_lines = newline_count + estimated_lines_from_width;
        estimated_lines.max(1)
    }

    fn save_changes(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ref record_id) = self.current_record_id {
            let raw_title = self
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

            let raw_content = self
                .content_input
                .as_ref()
                .map(|input| input.read(cx).value().to_string())
                .unwrap_or_else(|| self.record_content.clone());
            let parsed_fields = parsing::parse_record_fields(raw_title.as_deref(), &raw_content);
            let next_tags =
                parsing::reconcile_metadata(&self.tags, &self.inline_tags, &parsed_fields.tags);
            let next_persons = parsing::reconcile_metadata(
                &self.persons,
                &self.inline_persons,
                &parsed_fields.people,
            );
            self.record_title = parsed_fields.title.clone();
            self.record_content = parsed_fields.content.clone();
            self.tags = next_tags.clone();
            self.persons = next_persons.clone();
            self.inline_tags = parsed_fields.tags.clone();
            self.inline_persons = parsed_fields.people.clone();

            let payload = SavePayload {
                record_id: record_id.clone(),
                title: parsed_fields.title,
                content: parsed_fields.content,
                tags: next_tags,
                persons: next_persons,
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
        let content_expanded = self.content_expanded;

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
                    .h_full(),
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
                                    // 内容输入（多行文本区域）
                                    .child(
                                        v_flex()
                                            .gap(px(6.0))
                                            .child(
                                                h_flex()
                                                    .justify_between()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(rgb(0x666666))
                                                            .child("内容"),
                                                    )
                                                    .when(self.content_input.as_ref().map_or(false, |input| {
                                                        let content = input.read(cx).value();
                                                        Self::estimate_line_count(&content) > 6
                                                    }), |el| {
                                                        el.child(
                                                            Button::new("toggle-content-expand")
                                                                .child(if content_expanded { "收起" } else { "展开" })
                                                                .text_color(rgb(0x1890ff))
                                                                .on_click(cx.listener(move |this, _event: &ClickEvent, window, cx| {
                                                                    this.toggle_content_expanded(window, cx);
                                                                    cx.stop_propagation();
                                                                })),
                                                        )
                                                    }),
                                            )
                                            .when_some(content_input_clone.clone(), |el, input| {
                                                let content = input.read(cx).value();
                                                let line_count = Self::estimate_line_count(&content);
                                                let needs_scroll = line_count > 6 && !content_expanded;
                                                let is_expanded = content_expanded;

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
                                                        .when(!needs_scroll && !is_expanded, |d| {
                                                            d.h_auto()
                                                        })
                                                        .when(needs_scroll, |d| {
                                                            d.h(px(144.0))
                                                        })
                                                        .when(is_expanded, |d| {
                                                            let total_height = ((line_count as f32) * 20.0 + 16.0).max(144.0);
                                                            d.h(px(total_height))
                                                        })
                                                        .child(
                                                            Input::new(&input)
                                                                .appearance(false)
                                                                .text_size(px(14.0))
                                                                .when(needs_scroll || is_expanded, |i| i.h_full()),
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
                                h_flex()
                                    .gap(px(8.0))
                                    .child(
                                        div().flex_1().child(
                                            Button::new("record-sidebar-delete-detail")
                                                .w_full()
                                                .child("删除")
                                                .text_color(rgb(0xff4d4f))
                                                .on_click(cx.listener(
                                                    |this, _event, _window, cx| {
                                                        if let Some(ref record_id) =
                                                            this.current_record_id
                                                        {
                                                            if let Some(ref callback) =
                                                                this.on_delete
                                                            {
                                                                callback(record_id.clone(), cx);
                                                            }
                                                        }
                                                    },
                                                )),
                                        ),
                                    )
                                    .child(
                                        div().flex_1().child(
                                            Button::new("sidebar-save-detail")
                                                .w_full()
                                                .child("保存修改")
                                                .on_click(cx.listener(
                                                    |this, _event, window, cx| {
                                                        this.save_changes(window, cx);
                                                    },
                                                )),
                                        ),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }
}
