use crate::models::{Record, RecordType, TaskStatus};
use crate::store::Store;
use chrono::{DateTime, Datelike, Duration, Local, Utc, Weekday};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use gpui_component::{h_flex, v_flex};
use std::collections::HashSet;

const PAGE_SIZE: usize = 50;
const TIMELINE_CONTENT_LIMIT: usize = 52;

pub struct Timeline {
    store: Store,
    records: Vec<Record>,
    selected_tags: HashSet<String>,
    available_tags: Vec<String>,
    is_loading: bool,
    has_more: bool,
    offset: usize,
    _window_activation_subscription: Subscription,
}

impl Timeline {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut panel = Self {
            store,
            records: Vec::new(),
            selected_tags: HashSet::new(),
            available_tags: vec!["全部".to_string()],
            is_loading: false,
            has_more: true,
            offset: 0,
            _window_activation_subscription: cx.observe_window_activation(
                window,
                |this, window, cx| {
                    if window.is_window_active() {
                        this.refresh_data(cx);
                    }
                },
            ),
        };
        panel.load_data(cx);
        panel.load_available_tags(cx);
        panel
    }

    fn refresh_data(&mut self, cx: &mut Context<Self>) {
        self.records.clear();
        self.offset = 0;
        self.has_more = true;
        self.load_data(cx);
        self.load_available_tags(cx);
    }

    fn load_data(&mut self, cx: &mut Context<Self>) {
        if self.is_loading || !self.has_more {
            return;
        }

        self.is_loading = true;
        let store = self.store.clone();
        let offset = self.offset;
        let limit = PAGE_SIZE;

        cx.spawn(async move |view, cx| {
            match store.get_timeline(limit, offset).await {
                Ok(new_records) => {
                    let has_more = new_records.len() == limit;
                    let _ = view.update(cx, |panel, cx| {
                        panel.is_loading = false;
                        panel.has_more = has_more;
                        panel.offset += new_records.len();

                        // 合并记录并按时间倒序排序
                        panel.records.extend(new_records);
                        panel
                            .records
                            .sort_by(|a, b| b.created_at.cmp(&a.created_at));
                        cx.notify();
                    });
                }
                Err(e) => {
                    eprintln!("[Timeline] Failed to load records: {}", e);
                    let _ = view.update(cx, |panel, cx| {
                        panel.is_loading = false;
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    fn load_available_tags(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();

        cx.spawn(async move |view, cx| {
            match store.get_all_tags().await {
                Ok(tags) => {
                    let _ = view.update(cx, |panel, cx| {
                        // 保留 "全部" 选项，添加从数据库加载的标签
                        let mut tag_names = vec!["全部".to_string()];
                        for tag in tags {
                            tag_names.push(tag.name);
                        }
                        panel.available_tags = tag_names;
                        cx.notify();
                    });
                }
                Err(e) => {
                    eprintln!("[Timeline] Failed to load tags: {}", e);
                }
            }
        })
        .detach();
    }

    fn toggle_tag(&mut self, tag: &str, cx: &mut Context<Self>) {
        if tag == "全部" {
            self.selected_tags.clear();
        } else {
            if self.selected_tags.contains(tag) {
                self.selected_tags.remove(tag);
            } else {
                self.selected_tags.insert(tag.to_string());
            }
        }
        self.refresh_data(cx);
        cx.notify();
    }

    fn get_node_icon(record: &Record) -> &'static str {
        match record.record_type {
            RecordType::Task => match record.status {
                Some(TaskStatus::InProgress) => "▶",
                Some(TaskStatus::Done) => "☑",
                Some(TaskStatus::Cancelled) => "✕",
                _ => "◎",
            },
            RecordType::Event => "●",
            RecordType::Idea => "💡",
            RecordType::Note => "📝",
        }
    }

    fn get_node_color(record: &Record) -> Rgba {
        match record.record_type {
            RecordType::Task => match record.status {
                Some(TaskStatus::InProgress) => rgb(0x1890ff),
                Some(TaskStatus::Done) => rgb(0x52c41a),
                Some(TaskStatus::Cancelled) => rgb(0xff4d4f),
                _ => rgb(0x1890ff),
            },
            RecordType::Event => rgb(0x8c8c8c),
            RecordType::Idea => rgb(0xfaad14),
            RecordType::Note => rgb(0x722ed1),
        }
    }

    fn format_time(dt: DateTime<Utc>) -> String {
        let local = dt.with_timezone(&Local);
        let now = Local::now();
        let today = now.date_naive();
        let yesterday = today - Duration::days(1);
        let date = local.date_naive();

        if date == today {
            local.format("%H:%M").to_string()
        } else if date == yesterday {
            "昨天".to_string()
        } else if date.iso_week() == today.iso_week() && date.year() == today.year() {
            match local.weekday() {
                Weekday::Mon => "周一",
                Weekday::Tue => "周二",
                Weekday::Wed => "周三",
                Weekday::Thu => "周四",
                Weekday::Fri => "周五",
                Weekday::Sat => "周六",
                Weekday::Sun => "周日",
            }
            .to_string()
        } else if date.year() == today.year() {
            local.format("%-m月%-d日").to_string()
        } else {
            local.format("%Y年%-m月%-d日").to_string()
        }
    }

    fn get_record_type_label(record: &Record) -> &'static str {
        match record.record_type {
            RecordType::Task => "任务",
            RecordType::Event => "事件",
            RecordType::Idea => "想法",
            RecordType::Note => "笔记",
        }
    }

    fn get_status_label(record: &Record) -> Option<&'static str> {
        match record.record_type {
            RecordType::Task => match record.status {
                Some(TaskStatus::InProgress) => Some("进行中"),
                Some(TaskStatus::Done) => Some("已完成"),
                Some(TaskStatus::Cancelled) => Some("已取消"),
                _ => Some("待办"),
            },
            _ => None,
        }
    }

    fn normalize_text(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn truncate_text(text: &str, limit: usize) -> String {
        let normalized = Self::normalize_text(text);
        if normalized.chars().count() <= limit {
            normalized
        } else {
            format!("{}...", normalized.chars().take(limit).collect::<String>())
        }
    }

    fn record_summary(record: &Record) -> String {
        let text = record
            .title
            .clone()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| record.content.clone());
        Self::truncate_text(&text, TIMELINE_CONTENT_LIMIT)
    }

    fn render_tag_filter(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tags = self.available_tags.clone();
        let selected = self.selected_tags.clone();

        div()
            .flex()
            .flex_wrap()
            .gap(px(8.0))
            .children(tags.into_iter().enumerate().map(move |(idx, tag)| {
                let is_selected = if tag == "全部" {
                    selected.is_empty()
                } else {
                    selected.contains(&tag)
                };

                let tag_clone = tag.clone();
                div()
                    .id(("tag", idx))
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded(px(16.0))
                    .cursor_pointer()
                    .border_1()
                    .border_color(if is_selected {
                        rgb(0x1890ff)
                    } else {
                        rgb(0xd9d9d9)
                    })
                    .bg(if is_selected {
                        rgb(0xe6f7ff)
                    } else {
                        rgb(0xffffff)
                    })
                    .text_color(if is_selected {
                        rgb(0x1890ff)
                    } else {
                        rgb(0x595959)
                    })
                    .text_sm()
                    .hover(|s| {
                        s.bg(if is_selected {
                            rgb(0xbae7ff)
                        } else {
                            rgb(0xf5f5f5)
                        })
                    })
                    .child(tag.clone())
                    .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                        this.toggle_tag(&tag_clone, cx);
                    }))
            }))
            .child(
                div()
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded(px(16.0))
                    .cursor_pointer()
                    .border_1()
                    .border_color(rgb(0xd9d9d9))
                    .bg(rgb(0xffffff))
                    .text_color(rgb(0x8c8c8c))
                    .text_sm()
                    .hover(|s| s.bg(rgb(0xf5f5f5)))
                    .child("+固定"),
            )
    }

    fn render_timeline_item(
        &self,
        record: &Record,
        is_last: bool,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let time_str = Self::format_time(record.created_at);
        let icon = Self::get_node_icon(record);
        let icon_color = Self::get_node_color(record);
        let record_type_label = Self::get_record_type_label(record);
        let status_label = Self::get_status_label(record);
        let content = Self::record_summary(record);
        let tags = record.tags.clone();

        h_flex()
            .w_full()
            .child(
                div()
                    .w(px(80.0))
                    .flex()
                    .justify_end()
                    .pr(px(12.0))
                    .text_sm()
                    .text_color(rgb(0x8c8c8c))
                    .child(time_str),
            )
            .child(
                div()
                    .w(px(24.0))
                    .relative()
                    .child(
                        div()
                            .absolute()
                            .left(px(11.0))
                            .top(px(0.0))
                            .bottom(if is_last { px(0.0) } else { px(-1000.0) })
                            .w(px(2.0))
                            .bg(rgb(0xe8e8e8)),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(0.0))
                            .top(px(2.0))
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(icon_color)
                            .text_sm()
                            .child(icon),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .pb(px(24.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        h_flex()
                            .gap(px(8.0))
                            .items_center()
                            .child(
                                div()
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .bg(rgb(0xf0f0f0))
                                    .text_xs()
                                    .text_color(rgb(0x8c8c8c))
                                    .child(record_type_label),
                            )
                            .children(status_label.map(|status| {
                                div()
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .bg(match record.status {
                                        Some(TaskStatus::InProgress) => rgb(0xe6f7ff),
                                        Some(TaskStatus::Done) => rgb(0xf6ffed),
                                        Some(TaskStatus::Cancelled) => rgb(0xfff1f0),
                                        _ => rgb(0xf5f5f5),
                                    })
                                    .text_xs()
                                    .text_color(match record.status {
                                        Some(TaskStatus::InProgress) => rgb(0x1890ff),
                                        Some(TaskStatus::Done) => rgb(0x52c41a),
                                        Some(TaskStatus::Cancelled) => rgb(0xff4d4f),
                                        _ => rgb(0x8c8c8c),
                                    })
                                    .child(status)
                            })),
                    )
                    .child(
                        div()
                            .text_base()
                            .text_color(rgb(0x262626))
                            .line_height(relative(1.35))
                            .child(content),
                    )
                    .child(h_flex().gap(px(6.0)).flex_wrap().children(
                        tags.into_iter().enumerate().map(|(idx, tag)| {
                            div()
                                .id(("item-tag", idx))
                                .px(px(6.0))
                                .py(px(2.0))
                                .rounded(px(4.0))
                                .bg(rgb(0xf5f5f5))
                                .text_xs()
                                .text_color(rgb(0x595959))
                                .child(format!("#{}", tag))
                        }),
                    )),
            )
    }
}

impl Render for Timeline {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let records = self.records.clone();
        let is_loading = self.is_loading;
        let has_more = self.has_more;

        v_flex()
            .size_full()
            .gap(px(16.0))
            .p(px(16.0))
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0x262626))
                    .child(format!("时间线 ({} 条记录)", records.len())),
            )
            .child(self.render_tag_filter(cx))
            .child(
                div().flex_1().overflow_hidden().child(
                    div()
                        .id("timeline-list")
                        .size_full()
                        .flex()
                        .flex_col()
                        .pr(px(16.0))
                        .overflow_y_scrollbar()
                        .children(records.iter().enumerate().map(|(idx, record)| {
                            let is_last = idx == records.len() - 1;
                            self.render_timeline_item(record, is_last, cx)
                        }))
                        .child(
                            div()
                                .w_full()
                                .py(px(16.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(is_loading, |el| {
                                    el.child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(0x8c8c8c))
                                            .child("加载中..."),
                                    )
                                })
                                .when(!is_loading && has_more, |el| {
                                    el.child(
                                        div()
                                            .cursor_pointer()
                                            .px(px(16.0))
                                            .py(px(8.0))
                                            .rounded(px(6.0))
                                            .bg(rgb(0xf5f5f5))
                                            .hover(|s| s.bg(rgb(0xe8e8e8)))
                                            .text_sm()
                                            .text_color(rgb(0x595959))
                                            .child("点击加载更多"),
                                    )
                                })
                                .when(!is_loading && !has_more && !records.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(0xbfbfbf))
                                            .child("— 没有更多记录了 —"),
                                    )
                                })
                                .when(records.is_empty() && !is_loading, |el| {
                                    el.child(
                                        div().text_sm().text_color(rgb(0xbfbfbf)).child("暂无记录"),
                                    )
                                }),
                        ),
                ),
            )
    }
}
