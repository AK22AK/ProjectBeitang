use crate::models::{Priority, Record, RecordType, TaskStatus};
use crate::store::{DashboardData, Store};
use chrono::{Datelike, Duration, Local};
use gpui::*;
use gpui_component::h_flex;

const DASHBOARD_PENDING_TITLE_LIMIT: usize = 30;
const DASHBOARD_IN_PROGRESS_LIMIT: usize = 28;
const DASHBOARD_RECENT_LIMIT: usize = 30;
const DASHBOARD_STATUS_WIDTH: Pixels = px(92.0);

pub struct Dashboard {
    store: Store,
    dashboard_data: Option<DashboardData>,
    common_tags: Vec<String>,
    focus_handle: FocusHandle,
    _window_activation_subscription: Subscription,
}

impl Dashboard {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let mut dashboard = Self {
            store,
            dashboard_data: None,
            common_tags: Vec::new(),
            focus_handle,
            _window_activation_subscription: cx.observe_window_activation(
                window,
                |this, window, cx| {
                    if window.is_window_active() {
                        this.load_data(cx);
                        this.load_common_tags(cx);
                    }
                },
            ),
        };
        dashboard.load_data(cx);
        dashboard.load_common_tags(cx);
        dashboard
    }

    fn load_data(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        cx.spawn(async move |view, cx| match store.get_dashboard().await {
            Ok(data) => {
                let _ = view.update(cx, |dashboard, cx| {
                    dashboard.dashboard_data = Some(data);
                    cx.notify();
                });
            }
            Err(e) => {
                eprintln!("[Dashboard] Failed to load data: {}", e);
            }
        })
        .detach();
    }

    fn load_common_tags(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            match store.get_all_tags().await {
                Ok(tags) => {
                    let _ = view.update(cx, |dashboard, cx| {
                        // Take top 5 tags by usage count (for now just take first 5)
                        dashboard.common_tags = tags.into_iter().take(5).map(|t| t.name).collect();
                        cx.notify();
                    });
                }
                Err(e) => {
                    eprintln!("[Dashboard] Failed to load tags: {}", e);
                }
            }
        })
        .detach();
    }

    fn on_tag_click(&mut self, tag: &str, cx: &mut Context<Self>) {
        // Navigate to timeline with tag filter
        // This would typically emit an event or call a callback to switch panels
        // For now, we'll just log it
        eprintln!(
            "[Dashboard] Tag clicked: #{} - would navigate to filtered view",
            tag
        );
        cx.notify();
    }

    fn sort_by_quadrant(tasks: &mut Vec<Record>) {
        tasks.sort_by(|a, b| {
            let a_score = Self::quadrant_score(a);
            let b_score = Self::quadrant_score(b);
            a_score.cmp(&b_score)
        });
    }

    fn quadrant_score(task: &Record) -> u8 {
        let now = Local::now();
        let today = now.date_naive();

        let is_urgent = task
            .due_date
            .map(|dt| {
                let due = dt.with_timezone(&Local).date_naive();
                due <= today + Duration::days(1)
            })
            .unwrap_or(false);

        let is_important = matches!(task.priority, Some(Priority::High));

        match (is_important, is_urgent) {
            (true, true) => 1,
            (true, false) => 2,
            (false, true) => 3,
            (false, false) => 4,
        }
    }

    fn priority_mark(priority: &Option<Priority>) -> &'static str {
        match priority {
            Some(Priority::High) => "!!",
            Some(Priority::Medium) => "!",
            Some(Priority::Low) | None => "",
        }
    }

    fn priority_color(priority: &Option<Priority>) -> Rgba {
        match priority {
            Some(Priority::High) => rgb(0xff4d4f),
            Some(Priority::Medium) => rgb(0xffaa00),
            Some(Priority::Low) | None => rgb(0x52c41a),
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

    fn display_text(record: &Record, limit: usize) -> String {
        let base = record
            .title
            .clone()
            .filter(|title| !title.trim().is_empty())
            .or_else(|| {
                record
                    .content
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .map(|line| line.trim().to_string())
            })
            .unwrap_or_else(|| "无标题".to_string());
        Self::truncate_text(&base, limit)
    }

    fn format_relative_time(dt: chrono::DateTime<chrono::Utc>) -> String {
        let local = dt.with_timezone(&Local);
        let now = Local::now();
        let today = now.date_naive();
        let yesterday = today - Duration::days(1);
        let dt_date = local.date_naive();

        if dt_date == today {
            format!("今天 {}", local.format("%H:%M"))
        } else if dt_date == yesterday {
            format!("昨天 {}", local.format("%H:%M"))
        } else if dt_date >= today - Duration::days(6) {
            let weekday = match local.weekday() {
                chrono::Weekday::Mon => "周一",
                chrono::Weekday::Tue => "周二",
                chrono::Weekday::Wed => "周三",
                chrono::Weekday::Thu => "周四",
                chrono::Weekday::Fri => "周五",
                chrono::Weekday::Sat => "周六",
                chrono::Weekday::Sun => "周日",
            };
            format!("{} {}", weekday, local.format("%H:%M"))
        } else {
            local.format("%m-%d %H:%M").to_string()
        }
    }

    fn format_duration(start: chrono::DateTime<chrono::Utc>) -> String {
        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(start);
        let hours = duration.num_hours();
        let minutes = duration.num_minutes() % 60;

        if hours > 0 {
            format!("{}小时{}分", hours, minutes)
        } else {
            format!("{}分钟", minutes)
        }
    }

    fn record_type_icon(record: &Record) -> &'static str {
        if record.record_type == RecordType::Task {
            if record.completed_at.is_some() {
                "☑"
            } else if record.status == Some(TaskStatus::InProgress) {
                "▶"
            } else {
                "◎"
            }
        } else if record.record_type == RecordType::Idea {
            "💡"
        } else {
            "📝"
        }
    }

    fn render_section_header(title: &str) -> impl IntoElement {
        div()
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(0x666666))
            .mb(px(12.0))
            .child(title.to_string())
    }

    fn render_empty_state(message: &str) -> impl IntoElement {
        div()
            .text_sm()
            .text_color(rgb(0xaaaaaa))
            .py(px(16.0))
            .child(message.to_string())
    }
}

impl Focusable for Dashboard {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Dashboard {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let data = self.dashboard_data.clone();

        let (in_progress, pending_tasks, recent_records, total_pending) = data
            .map(|d| {
                let pending_count = d.total_pending;
                (
                    d.in_progress,
                    d.pending_tasks,
                    d.recent_records,
                    pending_count,
                )
            })
            .unwrap_or_default();

        let mut sorted_pending = pending_tasks.clone();
        Self::sort_by_quadrant(&mut sorted_pending);
        let display_pending: Vec<_> = sorted_pending.iter().take(5).cloned().collect();
        let remaining_count = total_pending.saturating_sub(5);

        let display_records: Vec<_> = recent_records.iter().take(6).cloned().collect();

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .track_focus(&self.focus_handle(cx))
            .child(
                div()
                    .px(px(16.0))
                    .py(px(12.0))
                    .bg(rgb(0xf8f9fa))
                    .border_1()
                    .border_color(rgb(0xe8e8e8))
                    .rounded(px(8.0))
                    .child(Self::render_section_header("进行中"))
                    .children(if in_progress.is_empty() {
                        vec![Self::render_empty_state("暂无进行中的任务").into_any_element()]
                    } else {
                        in_progress
                            .iter()
                            .enumerate()
                            .map(|(idx, task)| {
                                let start_time = task.updated_at;
                                div()
                                    .id(("in-progress", idx))
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .py(px(8.0))
                                    .px(px(12.0))
                                    .bg(rgb(0xffffff))
                                    .rounded(px(6.0))
                                    .border_1()
                                    .border_color(rgb(0xe0e0e0))
                                    .hover(|s| s.bg(rgb(0xf0f5ff)))
                                    .cursor_pointer()
                                    .child(
                                        h_flex()
                                            .gap(px(8.0))
                                            .items_center()
                                            .child(div().text_base().child("▶"))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .text_base()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(Self::display_text(
                                                        task,
                                                        DASHBOARD_IN_PROGRESS_LIMIT,
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .w(px(80.0))
                                                    .text_right()
                                                    .text_sm()
                                                    .text_color(rgb(0x1890ff))
                                                    .child(Self::format_duration(start_time)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(0x999999))
                                            .ml(px(24.0))
                                            .child(format!(
                                                "开始于 {}",
                                                start_time.with_timezone(&Local).format("%H:%M")
                                            )),
                                    )
                                    .into_any_element()
                            })
                            .collect()
                    }),
            )
            .child(
                div()
                    .px(px(16.0))
                    .py(px(12.0))
                    .bg(rgb(0xf8f9fa))
                    .border_1()
                    .border_color(rgb(0xe8e8e8))
                    .rounded(px(8.0))
                    .child(Self::render_section_header("待办"))
                    .children(if display_pending.is_empty() {
                        vec![Self::render_empty_state("暂无待办任务").into_any_element()]
                    } else {
                        let mut elements: Vec<AnyElement> = Vec::new();

                        for (idx, task) in display_pending.iter().enumerate() {
                            let priority = &task.priority;
                            let priority_mark = Self::priority_mark(priority);
                            let is_in_progress = task.status == Some(TaskStatus::InProgress);

                            let status_element = if is_in_progress {
                                div()
                                    .w(DASHBOARD_STATUS_WIDTH)
                                    .flex()
                                    .justify_end()
                                    .child(
                                        div()
                                            .min_w(px(82.0))
                                            .text_center()
                                            .text_xs()
                                            .px(px(6.0))
                                            .py(px(2.0))
                                            .bg(rgb(0xe6f7ff))
                                            .text_color(rgb(0x1890ff))
                                            .rounded(px(4.0))
                                            .child("进行中"),
                                    )
                                    .into_any_element()
                            } else if let Some(due) = task.due_date {
                                let due_local = due.with_timezone(&Local);
                                let now = Local::now();
                                let today = now.date_naive();
                                let due_date = due_local.date_naive();

                                let ddl_text = if due_date == today {
                                    "DDL今天".to_string()
                                } else if due_date == today + Duration::days(1) {
                                    "DDL明天".to_string()
                                } else {
                                    format!("DDL{}/{}", due_local.month(), due_local.day())
                                };

                                div()
                                    .w(DASHBOARD_STATUS_WIDTH)
                                    .flex()
                                    .justify_end()
                                    .child(
                                        div()
                                            .min_w(px(82.0))
                                            .text_center()
                                            .text_xs()
                                            .px(px(6.0))
                                            .py(px(2.0))
                                            .bg(if due_date <= today {
                                                rgb(0xfff2f0)
                                            } else {
                                                rgb(0xf6ffed)
                                            })
                                            .text_color(if due_date <= today {
                                                rgb(0xff4d4f)
                                            } else {
                                                rgb(0x52c41a)
                                            })
                                            .rounded(px(4.0))
                                            .child(ddl_text),
                                    )
                                    .into_any_element()
                            } else {
                                div().w(DASHBOARD_STATUS_WIDTH).into_any_element()
                            };

                            elements.push(
                                div()
                                    .id(("pending", idx))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .py(px(8.0))
                                    .px(px(12.0))
                                    .bg(rgb(0xffffff))
                                    .rounded(px(6.0))
                                    .border_1()
                                    .border_color(rgb(0xe0e0e0))
                                    .hover(|s| s.bg(rgb(0xf0f5ff)))
                                    .cursor_pointer()
                                    .child(
                                        h_flex()
                                            .flex_1()
                                            .gap(px(8.0))
                                            .items_center()
                                            .child(
                                                div()
                                                    .w(px(20.0))
                                                    .text_sm()
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(Self::priority_color(priority))
                                                    .child(priority_mark),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .overflow_hidden()
                                                    .text_sm()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(0x262626))
                                                    .child(Self::display_text(
                                                        task,
                                                        DASHBOARD_PENDING_TITLE_LIMIT,
                                                    )),
                                            ),
                                    )
                                    .child(status_element)
                                    .into_any_element(),
                            );
                        }

                        if remaining_count > 0 {
                            elements.push(
                                div()
                                    .flex()
                                    .justify_center()
                                    .py(px(8.0))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(0x1890ff))
                                            .cursor_pointer()
                                            .hover(|s| s.text_color(rgb(0x40a9ff)))
                                            .child(format!("── 还有 {} 个 ──", remaining_count)),
                                    )
                                    .into_any_element(),
                            );
                        }

                        elements
                    }),
            )
            .child(
                div()
                    .px(px(16.0))
                    .py(px(12.0))
                    .bg(rgb(0xf8f9fa))
                    .border_1()
                    .border_color(rgb(0xe8e8e8))
                    .rounded(px(8.0))
                    .child(Self::render_section_header("回顾"))
                    .children(if display_records.is_empty() {
                        vec![Self::render_empty_state("暂无最近记录").into_any_element()]
                    } else {
                        display_records
                            .iter()
                            .enumerate()
                            .map(|(idx, record)| {
                                let icon = Self::record_type_icon(record);
                                let is_completed = record.completed_at.is_some();

                                div()
                                    .id(("recent", idx))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .py(px(8.0))
                                    .px(px(12.0))
                                    .bg(rgb(0xffffff))
                                    .rounded(px(6.0))
                                    .border_1()
                                    .border_color(rgb(0xe0e0e0))
                                    .hover(|s| s.bg(rgb(0xf0f5ff)))
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .text_base()
                                            .text_color(if is_completed {
                                                rgb(0x52c41a)
                                            } else {
                                                rgb(0x666666)
                                            })
                                            .child(icon),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .overflow_hidden()
                                            .text_sm()
                                            .child(Self::display_text(
                                                record,
                                                DASHBOARD_RECENT_LIMIT,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .w(px(88.0))
                                            .text_right()
                                            .text_xs()
                                            .text_color(rgb(0x999999))
                                            .child(Self::format_relative_time(record.created_at)),
                                    )
                                    .into_any_element()
                            })
                            .collect()
                    }),
            )
            .child(
                div()
                    .mt_auto()
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(rgb(0xe8e8e8))
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        h_flex()
                            .gap(px(8.0))
                            .items_center()
                            .child(div().text_sm().text_color(rgb(0x666666)).child("常用:"))
                            .children(self.common_tags.iter().enumerate().map(|(idx, tag)| {
                                let tag_clone = tag.clone();
                                div()
                                    .id(("dashboard-tag", idx))
                                    .text_sm()
                                    .text_color(rgb(0x1890ff))
                                    .cursor_pointer()
                                    .hover(|s| s.text_color(rgb(0x40a9ff)))
                                    .child(format!("#{}", tag))
                                    .on_click(cx.listener(
                                        move |this, _event: &ClickEvent, _window, cx| {
                                            this.on_tag_click(&tag_clone, cx);
                                        },
                                    ))
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x666666))
                            .cursor_pointer()
                            .hover(|s| s.text_color(rgb(0x1890ff)))
                            .child("统计 →"),
                    ),
            )
    }
}
