use crate::models::{Record, RecordType, TaskStatus, TimelineQuery};
use crate::store::Store;
use crate::ui::record_detail_sidebar::{RecordDetailSidebar, SavePayload as RecordSavePayload};
use crate::ui::task_detail_sidebar::{SavePayload as TaskSavePayload, TaskDetailSidebar};
use crate::ui::tokenized_text::{
    render_metadata_chip, render_tokenized_text, MetadataChipKind, TokenTextStyle,
};
use chrono::{DateTime, Datelike, Duration, Local, Utc, Weekday};
use gpui::prelude::FluentBuilder as _;
use gpui::StatefulInteractiveElement as _;
use gpui::*;
use gpui_component::button::Button;
use gpui_component::scroll::ScrollableElement;
use gpui_component::{h_flex, v_flex};
use std::collections::BTreeSet;
use uuid::Uuid;

const PAGE_SIZE: usize = 50;
const TIMELINE_CONTENT_LIMIT: usize = 52;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TimelineDetailTarget {
    Task,
    Record,
}

#[derive(Clone)]
struct PendingDeletion {
    id: Uuid,
    record_label: &'static str,
    display_title: String,
}

pub struct Timeline {
    store: Store,
    records: Vec<Record>,
    selected_tags: BTreeSet<String>,
    available_tags: Vec<String>,
    selected_persons: BTreeSet<String>,
    available_persons: Vec<String>,
    is_loading: bool,
    has_more: bool,
    offset: usize,
    request_generation: usize,
    focus_handle: FocusHandle,
    pending_deletion: Option<PendingDeletion>,
    task_detail_sidebar: Entity<TaskDetailSidebar>,
    record_detail_sidebar: Entity<RecordDetailSidebar>,
    _window_activation_subscription: Subscription,
}

impl Timeline {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let task_sidebar_store = store.clone();
        let record_sidebar_store = store.clone();

        let mut panel = Self {
            store,
            records: Vec::new(),
            selected_tags: BTreeSet::new(),
            available_tags: vec!["全部".to_string()],
            selected_persons: BTreeSet::new(),
            available_persons: vec!["全部".to_string()],
            is_loading: false,
            has_more: true,
            offset: 0,
            request_generation: 0,
            focus_handle,
            pending_deletion: None,
            task_detail_sidebar: cx
                .new(|cx| TaskDetailSidebar::new(task_sidebar_store.clone(), window, cx)),
            record_detail_sidebar: cx
                .new(|cx| RecordDetailSidebar::new(record_sidebar_store.clone(), window, cx)),
            _window_activation_subscription: cx.observe_window_activation(
                window,
                |this, window, cx| {
                    if window.is_window_active() {
                        this.refresh_data(cx);
                    }
                },
            ),
        };

        let handle = cx.entity().clone();
        panel.task_detail_sidebar.update(cx, |sidebar, _cx| {
            sidebar.on_save(move |payload, cx| {
                handle.update(cx, |panel, cx| {
                    panel.handle_task_sidebar_save(&payload, cx);
                });
            });
        });
        let handle = cx.entity().clone();
        panel.task_detail_sidebar.update(cx, |sidebar, _cx| {
            sidebar.on_delete(move |task_id, cx| {
                if let Ok(task_id) = Uuid::parse_str(&task_id) {
                    handle.update(cx, |panel, cx| {
                        panel.request_delete_record(task_id, cx);
                    });
                }
            });
        });
        let handle = cx.entity().clone();
        panel.task_detail_sidebar.update(cx, |sidebar, _cx| {
            sidebar.on_close(move |cx| {
                handle.update(cx, |panel, cx| {
                    panel.handle_detail_sidebar_close(cx);
                });
            });
        });

        let handle = cx.entity().clone();
        panel.record_detail_sidebar.update(cx, |sidebar, _cx| {
            sidebar.on_save(move |payload, cx| {
                handle.update(cx, |panel, cx| {
                    panel.handle_record_sidebar_save(&payload, cx);
                });
            });
        });
        let handle = cx.entity().clone();
        panel.record_detail_sidebar.update(cx, |sidebar, _cx| {
            sidebar.on_delete(move |record_id, cx| {
                if let Ok(record_id) = Uuid::parse_str(&record_id) {
                    handle.update(cx, |panel, cx| {
                        panel.request_delete_record(record_id, cx);
                    });
                }
            });
        });
        let handle = cx.entity().clone();
        panel.record_detail_sidebar.update(cx, |sidebar, _cx| {
            sidebar.on_close(move |cx| {
                handle.update(cx, |panel, cx| {
                    panel.handle_detail_sidebar_close(cx);
                });
            });
        });

        panel.load_data(cx);
        panel.load_available_tags(cx);
        panel.load_available_persons(cx);
        panel
    }

    fn current_query(&self, limit: usize, offset: usize) -> TimelineQuery {
        TimelineQuery {
            limit,
            offset,
            tags: self.selected_tags.iter().cloned().collect(),
            persons: self.selected_persons.iter().cloned().collect(),
        }
    }

    fn refresh_data(&mut self, cx: &mut Context<Self>) {
        self.request_generation += 1;
        self.records.clear();
        self.offset = 0;
        self.has_more = true;
        self.is_loading = false;
        self.load_data(cx);
        self.load_available_tags(cx);
        self.load_available_persons(cx);
        cx.notify();
    }

    fn load_data(&mut self, cx: &mut Context<Self>) {
        if self.is_loading || !self.has_more {
            return;
        }

        self.is_loading = true;
        let generation = self.request_generation;
        let query = self.current_query(PAGE_SIZE, self.offset);
        let request_offset = query.offset;
        let store = self.store.clone();

        cx.spawn(
            async move |view, cx| match store.get_timeline(query).await {
                Ok(new_records) => {
                    let has_more = new_records.len() == PAGE_SIZE;
                    let _ = view.update(cx, |panel, cx| {
                        if panel.request_generation != generation {
                            return;
                        }

                        panel.is_loading = false;
                        panel.has_more = has_more;

                        if request_offset == 0 {
                            panel.offset = new_records.len();
                            panel.records = new_records;
                        } else {
                            panel.offset += new_records.len();
                            panel.records.extend(new_records);
                        }

                        panel
                            .records
                            .sort_by(|left, right| right.created_at.cmp(&left.created_at));
                        panel.sync_open_detail_visibility(cx);
                        cx.notify();
                    });
                }
                Err(e) => {
                    eprintln!("[Timeline] Failed to load records: {}", e);
                    let _ = view.update(cx, |panel, cx| {
                        if panel.request_generation != generation {
                            return;
                        }
                        panel.is_loading = false;
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn load_available_tags(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();

        cx.spawn(async move |view, cx| match store.get_all_tags().await {
            Ok(tags) => {
                let _ = view.update(cx, |panel, cx| {
                    let mut tag_names = vec!["全部".to_string()];
                    tag_names.extend(tags.into_iter().map(|tag| tag.name));
                    panel.available_tags = tag_names;
                    cx.notify();
                });
            }
            Err(e) => {
                eprintln!("[Timeline] Failed to load tags: {}", e);
            }
        })
        .detach();
    }

    fn load_available_persons(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();

        cx.spawn(async move |view, cx| match store.get_all_persons().await {
            Ok(persons) => {
                let _ = view.update(cx, |panel, cx| {
                    let mut person_names = vec!["全部".to_string()];
                    person_names.extend(persons.into_iter().map(|person| person.name));
                    panel.available_persons = person_names;
                    cx.notify();
                });
            }
            Err(e) => {
                eprintln!("[Timeline] Failed to load persons: {}", e);
            }
        })
        .detach();
    }

    fn toggle_tag(&mut self, tag: &str, cx: &mut Context<Self>) {
        if tag == "全部" {
            if self.selected_tags.is_empty() {
                return;
            }
            self.selected_tags.clear();
        } else if !self.selected_tags.remove(tag) {
            self.selected_tags.insert(tag.to_string());
        }

        self.refresh_data(cx);
    }

    fn toggle_person(&mut self, person: &str, cx: &mut Context<Self>) {
        if person == "全部" {
            if self.selected_persons.is_empty() {
                return;
            }
            self.selected_persons.clear();
        } else if !self.selected_persons.remove(person) {
            self.selected_persons.insert(person.to_string());
        }

        self.refresh_data(cx);
    }

    fn clear_tag_filters(&mut self, cx: &mut Context<Self>) {
        if self.selected_tags.is_empty() {
            return;
        }

        self.selected_tags.clear();
        self.refresh_data(cx);
    }

    fn clear_person_filters(&mut self, cx: &mut Context<Self>) {
        if self.selected_persons.is_empty() {
            return;
        }

        self.selected_persons.clear();
        self.refresh_data(cx);
    }

    fn has_active_filters(&self) -> bool {
        !self.selected_tags.is_empty() || !self.selected_persons.is_empty()
    }

    fn detail_target_for(record: &Record) -> TimelineDetailTarget {
        match record.record_type {
            RecordType::Task => TimelineDetailTarget::Task,
            RecordType::Note | RecordType::Idea | RecordType::Event => TimelineDetailTarget::Record,
        }
    }

    fn sidebar_visible(&self, cx: &App) -> bool {
        self.task_detail_sidebar
            .read(cx)
            .current_task_id()
            .is_some()
            || self
                .record_detail_sidebar
                .read(cx)
                .current_record_id()
                .is_some()
    }

    fn selected_task_id(&self, cx: &App) -> Option<String> {
        self.task_detail_sidebar
            .read(cx)
            .current_task_id()
            .map(|id| id.to_string())
    }

    fn selected_record_id(&self, cx: &App) -> Option<String> {
        self.record_detail_sidebar
            .read(cx)
            .current_record_id()
            .map(|id| id.to_string())
    }

    fn is_record_selected(
        record: &Record,
        selected_task_id: Option<&str>,
        selected_record_id: Option<&str>,
    ) -> bool {
        match Self::detail_target_for(record) {
            TimelineDetailTarget::Task => selected_task_id == Some(record.id.to_string().as_str()),
            TimelineDetailTarget::Record => {
                selected_record_id == Some(record.id.to_string().as_str())
            }
        }
    }

    fn close_detail_sidebars(&mut self, cx: &mut Context<Self>) {
        self.task_detail_sidebar.update(cx, |sidebar, cx| {
            sidebar.dismiss(cx);
        });
        self.record_detail_sidebar.update(cx, |sidebar, cx| {
            sidebar.dismiss(cx);
        });
    }

    fn handle_detail_sidebar_close(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    fn refresh_available_metadata(&mut self, cx: &mut Context<Self>) {
        self.load_available_tags(cx);
        self.load_available_persons(cx);
    }

    fn select_record(&mut self, record: &Record, window: &mut Window, cx: &mut Context<Self>) {
        match Self::detail_target_for(record) {
            TimelineDetailTarget::Task => {
                self.record_detail_sidebar.update(cx, |sidebar, cx| {
                    sidebar.dismiss(cx);
                });
                self.task_detail_sidebar.update(cx, |sidebar, cx| {
                    sidebar.show_task(record, window, cx);
                });
            }
            TimelineDetailTarget::Record => {
                self.task_detail_sidebar.update(cx, |sidebar, cx| {
                    sidebar.dismiss(cx);
                });
                self.record_detail_sidebar.update(cx, |sidebar, cx| {
                    sidebar.show_record(record, window, cx);
                });
            }
        }

        cx.notify();
    }

    fn sync_open_detail_visibility(&mut self, cx: &mut Context<Self>) {
        let current_task_id = self.selected_task_id(cx);
        let current_record_id = self.selected_record_id(cx);
        let should_keep_task = current_task_id.as_ref().is_some_and(|task_id| {
            self.records.iter().any(|record| {
                matches!(Self::detail_target_for(record), TimelineDetailTarget::Task)
                    && record.id.to_string() == *task_id
            })
        });
        let should_keep_record = current_record_id.as_ref().is_some_and(|record_id| {
            self.records.iter().any(|record| {
                matches!(
                    Self::detail_target_for(record),
                    TimelineDetailTarget::Record
                ) && record.id.to_string() == *record_id
            })
        });

        let mut changed = false;
        if current_task_id.is_some() && !should_keep_task {
            self.task_detail_sidebar.update(cx, |sidebar, cx| {
                sidebar.dismiss(cx);
            });
            changed = true;
        }
        if current_record_id.is_some() && !should_keep_record {
            self.record_detail_sidebar.update(cx, |sidebar, cx| {
                sidebar.dismiss(cx);
            });
            changed = true;
        }

        if changed {
            cx.notify();
        }
    }

    fn handle_task_sidebar_save(&mut self, payload: &TaskSavePayload, cx: &mut Context<Self>) {
        let mut updated_task = None;
        let mut metadata_changed = false;

        if let Some(task) = self
            .records
            .iter_mut()
            .find(|record| record.id.to_string() == payload.task_id)
        {
            metadata_changed = task.tags != payload.tags || task.persons != payload.persons;
            task.title = payload.title.clone();
            task.content = payload.content.clone();
            task.priority = Some(payload.priority.clone());
            task.status = Some(payload.status.clone());
            task.due_date = payload.due_date;
            task.scheduled_for = payload.scheduled_for;
            task.cancelled_reason = payload.cancel_reason.clone();
            task.tags = payload.tags.clone();
            task.persons = payload.persons.clone();
            task.updated_at = chrono::Utc::now();

            match payload.status {
                TaskStatus::Done | TaskStatus::Cancelled => {
                    if task.completed_at.is_none() {
                        task.completed_at = Some(chrono::Utc::now());
                    }
                }
                _ => {
                    task.completed_at = None;
                }
            }

            updated_task = Some(task.clone());
        }

        let Some(updated_task) = updated_task else {
            return;
        };

        let should_refresh_results = metadata_changed && self.has_active_filters();
        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            if let Err(e) = store.update_record(updated_task).await {
                eprintln!("[Timeline] Failed to update task: {}", e);
                return;
            }

            let _ = view.update(cx, |panel, cx| {
                panel.refresh_available_metadata(cx);
                if should_refresh_results {
                    panel.refresh_data(cx);
                } else {
                    panel.sync_open_detail_visibility(cx);
                    cx.notify();
                }
            });
        })
        .detach();

        if !should_refresh_results {
            self.sync_open_detail_visibility(cx);
            cx.notify();
        }
    }

    fn handle_record_sidebar_save(&mut self, payload: &RecordSavePayload, cx: &mut Context<Self>) {
        let mut updated_record = None;
        let mut metadata_changed = false;

        if let Some(record) = self
            .records
            .iter_mut()
            .find(|item| item.id.to_string() == payload.record_id)
        {
            metadata_changed = record.tags != payload.tags || record.persons != payload.persons;
            record.title = payload.title.clone();
            record.content = payload.content.clone();
            record.tags = payload.tags.clone();
            record.persons = payload.persons.clone();
            record.updated_at = chrono::Utc::now();
            updated_record = Some(record.clone());
        }

        let Some(updated_record) = updated_record else {
            return;
        };

        let should_refresh_results = metadata_changed && self.has_active_filters();
        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            if let Err(e) = store.update_record(updated_record).await {
                eprintln!("[Timeline] Failed to update record: {}", e);
                return;
            }

            let _ = view.update(cx, |panel, cx| {
                panel.refresh_available_metadata(cx);
                if should_refresh_results {
                    panel.refresh_data(cx);
                } else {
                    panel.sync_open_detail_visibility(cx);
                    cx.notify();
                }
            });
        })
        .detach();

        if !should_refresh_results {
            self.sync_open_detail_visibility(cx);
            cx.notify();
        }
    }

    fn request_delete_record(&mut self, record_id: Uuid, cx: &mut Context<Self>) {
        if let Some(record) = self.records.iter().find(|record| record.id == record_id) {
            self.pending_deletion = Some(PendingDeletion {
                id: record_id,
                record_label: match Self::detail_target_for(record) {
                    TimelineDetailTarget::Task => "任务",
                    TimelineDetailTarget::Record => "记录",
                },
                display_title: record.display_title(),
            });
            cx.notify();
        }
    }

    fn cancel_delete_confirmation(&mut self, cx: &mut Context<Self>) {
        self.pending_deletion = None;
        cx.notify();
    }

    fn confirm_delete_record(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_deletion.clone() else {
            return;
        };

        self.perform_delete_record(pending.id, cx);
    }

    fn perform_delete_record(&mut self, record_id: Uuid, cx: &mut Context<Self>) {
        let store = self.store.clone();
        let record_id_string = record_id.to_string();
        cx.spawn(
            async move |view, cx| match store.delete_record(record_id).await {
                Ok(_) => {
                    let _ = view.update(cx, |panel, cx| {
                        panel.pending_deletion = None;

                        if panel.task_detail_sidebar.read(cx).current_task_id()
                            == Some(record_id_string.as_str())
                        {
                            panel.task_detail_sidebar.update(cx, |sidebar, cx| {
                                sidebar.dismiss(cx);
                            });
                        }

                        if panel.record_detail_sidebar.read(cx).current_record_id()
                            == Some(record_id_string.as_str())
                        {
                            panel.record_detail_sidebar.update(cx, |sidebar, cx| {
                                sidebar.dismiss(cx);
                            });
                        }

                        panel.refresh_available_metadata(cx);
                        panel.refresh_data(cx);
                    });
                }
                Err(e) => eprintln!("[Timeline] Failed to delete record: {}", e),
            },
        )
        .detach();
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

    fn render_filter_chip(
        id: impl Into<ElementId>,
        label: String,
        is_selected: bool,
    ) -> Stateful<Div> {
        div()
            .id(id)
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
            .hover(|style| {
                style.bg(if is_selected {
                    rgb(0xbae7ff)
                } else {
                    rgb(0xf5f5f5)
                })
            })
            .child(label)
    }

    fn render_tag_filter(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().gap(px(6.0)).child(
            h_flex()
                .gap(px(8.0))
                .flex_wrap()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0x595959))
                        .child("标签"),
                )
                .child(
                    Self::render_filter_chip(
                        "timeline-tag-all",
                        "全部".to_string(),
                        self.selected_tags.is_empty(),
                    )
                    .on_click(cx.listener(
                        |this, _event: &ClickEvent, _window, cx| {
                            this.toggle_tag("全部", cx);
                        },
                    )),
                )
                .children(
                    self.available_tags
                        .iter()
                        .enumerate()
                        .skip(1)
                        .map(|(idx, tag)| {
                            let is_selected = self.selected_tags.contains(tag);
                            let tag_clone = tag.clone();
                            Self::render_filter_chip(
                                ("timeline-tag-filter", idx),
                                format!("#{}", tag),
                                is_selected,
                            )
                            .on_click(cx.listener(
                                move |this, _event: &ClickEvent, _window, cx| {
                                    this.toggle_tag(&tag_clone, cx);
                                },
                            ))
                        }),
                )
                .when(!self.selected_tags.is_empty(), |el| {
                    el.child(Button::new("timeline-clear-tags").child("清除").on_click(
                        cx.listener(|this, _event, _window, cx| {
                            this.clear_tag_filters(cx);
                        }),
                    ))
                })
                .child(
                    div()
                        .id("timeline-pin-tag-placeholder")
                        .px(px(12.0))
                        .py(px(6.0))
                        .rounded(px(16.0))
                        .cursor_pointer()
                        .border_1()
                        .border_color(rgb(0xd9d9d9))
                        .bg(rgb(0xffffff))
                        .text_color(rgb(0x8c8c8c))
                        .text_sm()
                        .hover(|style| style.bg(rgb(0xf5f5f5)))
                        .child("+固定"),
                ),
        )
    }

    fn render_person_filter(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().gap(px(6.0)).child(
            h_flex()
                .gap(px(8.0))
                .flex_wrap()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0x595959))
                        .child("人物"),
                )
                .child(
                    Self::render_filter_chip(
                        "timeline-person-all",
                        "全部".to_string(),
                        self.selected_persons.is_empty(),
                    )
                    .on_click(cx.listener(
                        |this, _event: &ClickEvent, _window, cx| {
                            this.toggle_person("全部", cx);
                        },
                    )),
                )
                .children(
                    self.available_persons
                        .iter()
                        .enumerate()
                        .skip(1)
                        .map(|(idx, person)| {
                            let is_selected = self.selected_persons.contains(person);
                            let person_clone = person.clone();
                            Self::render_filter_chip(
                                ("timeline-person-filter", idx),
                                format!("@{}", person),
                                is_selected,
                            )
                            .on_click(cx.listener(
                                move |this, _event: &ClickEvent, _window, cx| {
                                    this.toggle_person(&person_clone, cx);
                                },
                            ))
                        }),
                )
                .when(!self.selected_persons.is_empty(), |el| {
                    el.child(
                        Button::new("timeline-clear-persons")
                            .child("清除")
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.clear_person_filters(cx);
                            })),
                    )
                }),
        )
    }

    fn render_filters(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap(px(10.0))
            .child(self.render_tag_filter(cx))
            .child(self.render_person_filter(cx))
    }

    fn render_timeline_item(
        &self,
        record: &Record,
        is_last: bool,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let time_str = Self::format_time(record.created_at);
        let icon = Self::get_node_icon(record);
        let icon_color = Self::get_node_color(record);
        let record_type_label = Self::get_record_type_label(record);
        let status_label = Self::get_status_label(record);
        let content = Self::record_summary(record);
        let tags = record.tags.clone();
        let persons = record.persons.clone();

        h_flex()
            .w_full()
            .min_w(px(0.0))
            .child(
                div()
                    .w(px(80.0))
                    .flex()
                    .justify_end()
                    .pr(px(12.0))
                    .pt(px(12.0))
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
                            .top(px(10.0))
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
                div().flex_1().min_w(px(0.0)).pb(px(20.0)).child(
                    div()
                        .id(record.id)
                        .w_full()
                        .min_w(px(0.0))
                        .p(px(12.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(if is_selected {
                            rgb(0x1890ff)
                        } else {
                            rgb(0xe8e8e8)
                        })
                        .bg(if is_selected {
                            rgb(0xe6f7ff)
                        } else {
                            rgb(0xffffff)
                        })
                        .cursor_pointer()
                        .hover(|style| {
                            style.bg(if is_selected {
                                rgb(0xe6f7ff)
                            } else {
                                rgb(0xf6ffed)
                            })
                        })
                        .on_click(cx.listener({
                            let record = record.clone();
                            move |this, _event: &ClickEvent, window, cx| {
                                this.select_record(&record, window, cx);
                                cx.stop_propagation();
                            }
                        }))
                        .child(
                            v_flex()
                                .gap(px(6.0))
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
                                        .child(render_tokenized_text(
                                            &content,
                                            TokenTextStyle::new(rgb(0x262626), FontWeight::NORMAL),
                                        )),
                                )
                                .when(!tags.is_empty(), |el| {
                                    el.child(h_flex().gap(px(6.0)).flex_wrap().children(
                                        tags.into_iter().enumerate().map(|(idx, tag)| {
                                            div().id(("timeline-item-tag", idx)).child(
                                                render_metadata_chip(MetadataChipKind::Tag, &tag),
                                            )
                                        }),
                                    ))
                                })
                                .when(!persons.is_empty(), |el| {
                                    el.child(h_flex().gap(px(6.0)).flex_wrap().children(
                                        persons.into_iter().enumerate().map(|(idx, person)| {
                                            div().id(("timeline-item-person", idx)).child(
                                                render_metadata_chip(
                                                    MetadataChipKind::Person,
                                                    &person,
                                                ),
                                            )
                                        }),
                                    ))
                                }),
                        ),
                ),
            )
    }

    fn render_delete_confirmation(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pending = self.pending_deletion.as_ref()?;
        let title = pending.display_title.clone();
        let record_label = pending.record_label;

        Some(
            div()
                .id("timeline-delete-confirm-overlay")
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(0xf5f5f5))
                .on_click(cx.listener(|_this, _event: &ClickEvent, _window, cx| {
                    cx.stop_propagation();
                }))
                .child(
                    div()
                        .w(px(360.0))
                        .max_w(px(360.0))
                        .p(px(20.0))
                        .rounded(px(12.0))
                        .bg(rgb(0xffffff))
                        .border_1()
                        .border_color(rgb(0xe8e8e8))
                        .shadow_lg()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .cursor_default()
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("删除{}", record_label)),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0x666666))
                                .child(format!("确认删除“{}”？删除后无法恢复。", title)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x999999))
                                .child("按 Enter 确认，按 Esc 取消"),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(8.0))
                                .child(
                                    Button::new("timeline-delete-confirm-cancel")
                                        .child("取消")
                                        .on_click(cx.listener(|this, _event, _window, cx| {
                                            this.cancel_delete_confirmation(cx);
                                        })),
                                )
                                .child(
                                    Button::new("timeline-delete-confirm-submit")
                                        .child("确认删除")
                                        .text_color(rgb(0xff4d4f))
                                        .on_click(cx.listener(|this, _event, _window, cx| {
                                            this.confirm_delete_record(cx);
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}

impl Render for Timeline {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.pending_deletion.is_some() {
            self.focus_handle.focus(window, cx);
        }

        let records = self.records.clone();
        let is_loading = self.is_loading;
        let has_more = self.has_more;
        let selected_task_id = self.selected_task_id(cx);
        let selected_record_id = self.selected_record_id(cx);

        v_flex()
            .size_full()
            .overflow_hidden()
            .relative()
            .p(px(24.0))
            .gap(px(18.0))
            .track_focus(&self.focus_handle(cx))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.pending_deletion.is_none() {
                    return;
                }

                match event.keystroke.key.as_str() {
                    "enter" => {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.confirm_delete_record(cx);
                    }
                    "escape" => {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.cancel_delete_confirmation(cx);
                    }
                    _ => {}
                }
            }))
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0x262626))
                    .child(format!("时间线 ({} 条记录)", records.len())),
            )
            .child(self.render_filters(cx))
            .child(
                div()
                    .id("timeline-main")
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                        if this.sidebar_visible(cx) {
                            this.close_detail_sidebars(cx);
                        }
                    }))
                    .child(
                        div()
                            .id("timeline-list")
                            .size_full()
                            .flex()
                            .flex_col()
                            .pr(px(16.0))
                            .overflow_y_scrollbar()
                            .children(records.iter().enumerate().map(|(idx, record)| {
                                let is_last = idx == records.len() - 1;
                                let is_selected = Self::is_record_selected(
                                    record,
                                    selected_task_id.as_deref(),
                                    selected_record_id.as_deref(),
                                );
                                self.render_timeline_item(record, is_last, is_selected, cx)
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
                                                .id("timeline-load-more")
                                                .cursor_pointer()
                                                .px(px(16.0))
                                                .py(px(8.0))
                                                .rounded(px(6.0))
                                                .bg(rgb(0xf5f5f5))
                                                .hover(|style| style.bg(rgb(0xe8e8e8)))
                                                .text_sm()
                                                .text_color(rgb(0x595959))
                                                .child("点击加载更多")
                                                .on_click(cx.listener(
                                                    |this, _event: &ClickEvent, _window, cx| {
                                                        this.load_data(cx);
                                                        cx.stop_propagation();
                                                    },
                                                )),
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
                                            div()
                                                .text_sm()
                                                .text_color(rgb(0xbfbfbf))
                                                .child("暂无记录"),
                                        )
                                    }),
                            ),
                    ),
            )
            .child(self.task_detail_sidebar.clone())
            .child(self.record_detail_sidebar.clone())
            .children(self.render_delete_confirmation(cx))
    }
}

impl Focusable for Timeline {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
