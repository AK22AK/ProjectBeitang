use crate::models::{Record, RecordType, TaskStatus};
use crate::store::Store;
use chrono::{DateTime, Datelike, Duration, Local, Utc, Weekday};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::button::Button;
use gpui_component::h_flex;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::v_flex;
use std::collections::HashSet;
use std::time::Duration as StdDuration;

const SEARCH_DEBOUNCE_MS: u64 = 300;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SearchFilterType {
    All,
    Task,
    Note,
    Idea,
}

impl SearchFilterType {
    fn label(&self) -> &'static str {
        match self {
            SearchFilterType::All => "全部",
            SearchFilterType::Task => "任务",
            SearchFilterType::Note => "记录",
            SearchFilterType::Idea => "想法",
        }
    }

    fn matches(&self, record_type: &RecordType) -> bool {
        match self {
            SearchFilterType::All => true,
            SearchFilterType::Task => matches!(record_type, RecordType::Task),
            SearchFilterType::Note => matches!(record_type, RecordType::Note),
            SearchFilterType::Idea => matches!(record_type, RecordType::Idea),
        }
    }
}

pub struct SearchPanel {
    store: Store,
    query: String,
    results: Vec<Record>,
    filter_type: SearchFilterType,
    selected_tags: HashSet<String>,
    #[allow(dead_code)]
    available_tags: Vec<String>,
    input_state: Entity<InputState>,
    focus_handle: FocusHandle,
    _search_subscription: Subscription,
    is_searching: bool,
}

impl SearchPanel {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| InputState::new(window, cx).placeholder("🔍 搜索内容..."));

        let _subscription = cx.subscribe_in(
            &input_state,
            window,
            |this, input_state, _event: &InputEvent, _window, cx| {
                let text = input_state.read(cx).text().to_string();
                this.on_query_change(text, cx);
            },
        );

        let focus_handle = cx.focus_handle();

        let mut panel = Self {
            store,
            query: String::new(),
            results: Vec::new(),
            filter_type: SearchFilterType::All,
            selected_tags: HashSet::new(),
            available_tags: Vec::new(),
            input_state,
            focus_handle,
            _search_subscription: _subscription,
            is_searching: false,
        };

        panel.load_available_tags(cx);
        panel
    }

    fn load_available_tags(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        cx.spawn(async move |view, cx| match store.get_all_tags().await {
            Ok(tags) => {
                let _ = view.update(cx, |panel, cx| {
                    panel.available_tags = tags.into_iter().map(|t| t.name).collect();
                    cx.notify();
                });
            }
            Err(e) => {
                eprintln!("[SearchPanel] Failed to load tags: {}", e);
            }
        })
        .detach();
    }

    fn on_query_change(&mut self, query: String, cx: &mut Context<Self>) {
        self.query = query;

        if self.query.len() < 2 {
            self.results.clear();
            cx.notify();
            return;
        }

        let store = self.store.clone();
        let search_query = self.query.clone();
        self.is_searching = true;
        cx.notify();

        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(StdDuration::from_millis(SEARCH_DEBOUNCE_MS))
                .await;

            match store.search_records(&search_query).await {
                Ok(records) => {
                    let _ = view.update(cx, |panel, cx| {
                        panel.results = records;
                        panel.is_searching = false;
                        cx.notify();
                    });
                }
                Err(e) => {
                    eprintln!("[SearchPanel] Search failed: {}", e);
                    let _ = view.update(cx, |panel, cx| {
                        panel.is_searching = false;
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    #[allow(dead_code)]
    fn perform_search(&mut self, cx: &mut Context<Self>) {
        if self.query.len() < 2 {
            return;
        }

        self.is_searching = true;
        let store = self.store.clone();
        let query = self.query.clone();

        cx.spawn(
            async move |view, cx| match store.search_records(&query).await {
                Ok(records) => {
                    let _ = view.update(cx, |panel, cx| {
                        panel.results = records;
                        panel.is_searching = false;
                        cx.notify();
                    });
                }
                Err(e) => {
                    eprintln!("[SearchPanel] Search failed: {}", e);
                    let _ = view.update(cx, |panel, cx| {
                        panel.is_searching = false;
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.clear();
        self.results.clear();
        self.input_state.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        cx.notify();
    }

    fn set_filter_type(
        &mut self,
        filter_type: SearchFilterType,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.filter_type = filter_type;
        cx.notify();
    }

    fn toggle_tag(&mut self, tag: &str, cx: &mut Context<Self>) {
        if self.selected_tags.contains(tag) {
            self.selected_tags.remove(tag);
        } else {
            self.selected_tags.insert(tag.to_string());
        }
        cx.notify();
    }

    fn clear_tag_filters(&mut self, cx: &mut Context<Self>) {
        self.selected_tags.clear();
        cx.notify();
    }

    fn get_filtered_results(&self) -> Vec<Record> {
        self.results
            .iter()
            .filter(|r| self.filter_type.matches(&r.record_type))
            .filter(|r| {
                if self.selected_tags.is_empty() {
                    return true;
                }
                r.tags.iter().any(|tag| self.selected_tags.contains(tag))
            })
            .cloned()
            .collect()
    }

    fn format_date_group(dt: DateTime<Utc>) -> String {
        let local = dt.with_timezone(&Local);
        let now = Local::now();
        let today = now.date_naive();
        let yesterday = today - Duration::days(1);
        let date = local.date_naive();

        if date == today {
            "今天".to_string()
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

    fn format_time(dt: DateTime<Utc>) -> String {
        let local = dt.with_timezone(&Local);
        local.format("%H:%M").to_string()
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

    fn get_record_type_label(record: &Record) -> &'static str {
        match record.record_type {
            RecordType::Task => "任务",
            RecordType::Event => "事件",
            RecordType::Idea => "想法",
            RecordType::Note => "笔记",
        }
    }

    fn highlight_match_text(&self, content: &str) -> impl IntoElement {
        if self.query.is_empty() || self.query.len() < 2 {
            return div()
                .text_base()
                .text_color(rgb(0x262626))
                .child(content.to_string())
                .into_any_element();
        }

        let query_lower = self.query.to_lowercase();
        let content_lower = content.to_lowercase();

        let mut elements: Vec<AnyElement> = Vec::new();
        let mut last_end = 0;

        for (idx, _) in content_lower.match_indices(&query_lower) {
            if idx > last_end {
                elements.push(
                    div()
                        .child(content[last_end..idx].to_string())
                        .into_any_element(),
                );
            }
            let end_idx = idx + self.query.len();
            elements.push(
                div()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0x1890ff))
                    .child(content[idx..end_idx].to_string())
                    .into_any_element(),
            );
            last_end = end_idx;
        }

        if last_end < content.len() {
            elements.push(
                div()
                    .child(content[last_end..].to_string())
                    .into_any_element(),
            );
        }

        if elements.is_empty() {
            return div()
                .text_base()
                .text_color(rgb(0x262626))
                .child(content.to_string())
                .into_any_element();
        }

        h_flex().flex_wrap().children(elements).into_any_element()
    }

    fn render_search_input(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap(px(8.0))
            .items_center()
            .child(div().flex_1().child(Input::new(&self.input_state)))
            .when(!self.query.is_empty(), |el| {
                el.child(
                    Button::new("clear-search")
                        .child("清除")
                        .on_click(cx.listener(|this, _event, window, cx| {
                            this.clear_search(window, cx);
                        })),
                )
            })
    }

    fn render_type_filter(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let filters = vec![
            SearchFilterType::All,
            SearchFilterType::Task,
            SearchFilterType::Note,
            SearchFilterType::Idea,
        ];

        h_flex().gap(px(4.0)).child(h_flex().gap(px(4.0)).children(
            filters.into_iter().enumerate().map(|(idx, filter)| {
                let is_selected = self.filter_type == filter;
                let filter_clone = filter;

                div()
                    .id(("filter", idx))
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
                    .child(filter.label())
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        this.set_filter_type(filter_clone, window, cx);
                    }))
            }),
        ))
    }

    fn render_tag_filter(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_selected = !self.selected_tags.is_empty();

        v_flex()
            .gap(px(8.0))
            .when(!self.available_tags.is_empty(), |el| {
                el.child(
                    h_flex()
                        .gap(px(4.0))
                        .flex_wrap()
                        .children(self.available_tags.iter().enumerate().map(|(idx, tag)| {
                            let is_selected = self.selected_tags.contains(tag);
                            let tag_clone = tag.clone();
                            div()
                                .id(("search-tag-filter", idx))
                                .px(px(10.0))
                                .py(px(4.0))
                                .rounded(px(12.0))
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
                                .child(format!("#{}", tag))
                                .on_click(cx.listener(
                                    move |this, _event: &ClickEvent, _window, cx| {
                                        this.toggle_tag(&tag_clone, cx);
                                    },
                                ))
                        }))
                        .when(has_selected, |el| {
                            el.child(Button::new("clear-search-tags").child("清除筛选").on_click(
                                cx.listener(|this, _event, _window, cx| {
                                    this.clear_tag_filters(cx);
                                }),
                            ))
                        }),
                )
            })
    }

    fn render_search_result_item(
        &self,
        record: &Record,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let time_str = Self::format_time(record.created_at);
        let icon = Self::get_node_icon(record);
        let icon_color = Self::get_node_color(record);
        let record_type_label = Self::get_record_type_label(record);
        let content = record.content.clone();
        let tags = record.tags.clone();

        h_flex()
            .w_full()
            .py(px(8.0))
            .gap(px(12.0))
            .child(
                div()
                    .w(px(60.0))
                    .text_sm()
                    .text_color(rgb(0x8c8c8c))
                    .child(time_str),
            )
            .child(div().text_color(icon_color).text_sm().child(icon))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        h_flex().gap(px(8.0)).items_center().child(
                            div()
                                .px(px(6.0))
                                .py(px(2.0))
                                .rounded(px(4.0))
                                .bg(rgb(0xf0f0f0))
                                .text_xs()
                                .text_color(rgb(0x8c8c8c))
                                .child(record_type_label),
                        ),
                    )
                    .child(self.highlight_match_text(&content))
                    .child(h_flex().gap(px(6.0)).flex_wrap().children(
                        tags.into_iter().enumerate().map(|(idx, tag)| {
                            div()
                                .id(("result-tag", idx))
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

    fn group_results_by_date(&self, results: &[Record]) -> Vec<(String, Vec<Record>)> {
        let mut groups: Vec<(String, Vec<Record>)> = Vec::new();
        let mut current_group: Option<(String, Vec<Record>)> = None;

        for record in results {
            let date_group = Self::format_date_group(record.created_at);

            match &mut current_group {
                Some((group_date, group_records)) if group_date == &date_group => {
                    group_records.push(record.clone());
                }
                _ => {
                    if let Some(group) = current_group.take() {
                        groups.push(group);
                    }
                    current_group = Some((date_group, vec![record.clone()]));
                }
            }
        }

        if let Some(group) = current_group {
            groups.push(group);
        }

        groups
    }

    fn render_results(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let filtered_results = self.get_filtered_results();
        let result_count = filtered_results.len();
        let grouped_results = self.group_results_by_date(&filtered_results);
        let is_searching = self.is_searching;
        let has_query = !self.query.is_empty();

        v_flex()
            .flex_1()
            .overflow_y_scrollbar()
            .child(div().py(px(8.0)).child(if is_searching {
                div().text_sm().text_color(rgb(0x8c8c8c)).child("搜索中...")
            } else if has_query {
                div()
                    .text_sm()
                    .text_color(rgb(0x595959))
                    .child(format!("找到 {} 个结果：", result_count))
            } else {
                div()
                    .text_sm()
                    .text_color(rgb(0x8c8c8c))
                    .child("输入关键词开始搜索")
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .children(grouped_results.into_iter().map(|(date_group, records)| {
                        v_flex()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .pt(px(16.0))
                                    .pb(px(8.0))
                                    .border_b_1()
                                    .border_color(rgb(0xe8e8e8))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(0x262626))
                                            .child(date_group),
                                    ),
                            )
                            .children(records.iter().enumerate().map(|(idx, record)| {
                                div()
                                    .id(("result", idx))
                                    .child(self.render_search_result_item(record, cx))
                            }))
                    }))
                    .when(
                        filtered_results.is_empty() && has_query && !is_searching,
                        |el| {
                            el.child(
                                div()
                                    .py(px(32.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(0xbfbfbf))
                                            .child("未找到匹配的结果"),
                                    ),
                            )
                        },
                    ),
            )
    }
}

impl Render for SearchPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .gap(px(16.0))
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0x262626))
                    .child("搜索"),
            )
            .child(self.render_search_input(cx))
            .child(self.render_type_filter(cx))
            .child(self.render_tag_filter(cx))
            .child(self.render_results(cx))
    }
}

impl Focusable for SearchPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
