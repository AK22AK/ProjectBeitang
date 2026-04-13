use crate::ai::{
    build_context_bundle, generate_summary, local_day_range_to_utc, AiContextBundle,
    AiContextQuery, AiSettings, AiSummaryMode,
};
use crate::platform::load_secret;
use crate::settings::load_app_settings;
use crate::store::Store;
use chrono::{Duration, Local, NaiveDate};
use gpui::{prelude::*, ClipboardItem, *};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::date_picker::{DatePicker, DatePickerState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::{Disableable, Sizable};
use std::collections::BTreeSet;

const AI_TWO_COLUMN_BREAKPOINT: Pixels = px(1280.0);

#[derive(Clone)]
struct AiRuntimeConfig {
    settings: AiSettings,
    has_api_key: bool,
}

pub struct AiPanel {
    store: Store,
    mode: AiSummaryMode,
    start_date_picker: Entity<DatePickerState>,
    end_date_picker: Entity<DatePickerState>,
    available_tags: Vec<String>,
    available_persons: Vec<String>,
    selected_tags: BTreeSet<String>,
    selected_persons: BTreeSet<String>,
    preview: Option<AiContextBundle>,
    config: AiRuntimeConfig,
    preview_loading: bool,
    generating: bool,
    result: Option<String>,
    notice: Option<String>,
    error: Option<String>,
    request_serial: usize,
    _window_activation_subscription: Subscription,
}

impl AiPanel {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let today = Local::now().date_naive();
        let start = today - Duration::days(6);
        let start_date_picker = cx.new(|cx| {
            let mut picker = DatePickerState::new(window, cx).date_format("%Y-%m-%d");
            picker.set_date(start, window, cx);
            picker
        });
        let end_date_picker = cx.new(|cx| {
            let mut picker = DatePickerState::new(window, cx).date_format("%Y-%m-%d");
            picker.set_date(today, window, cx);
            picker
        });

        let mut panel = Self {
            store,
            mode: AiSummaryMode::PastSummary,
            start_date_picker,
            end_date_picker,
            available_tags: Vec::new(),
            available_persons: Vec::new(),
            selected_tags: BTreeSet::new(),
            selected_persons: BTreeSet::new(),
            preview: None,
            config: Self::load_runtime_config(),
            preview_loading: false,
            generating: false,
            result: None,
            notice: None,
            error: None,
            request_serial: 0,
            _window_activation_subscription: cx.observe_window_activation(
                window,
                |this, window, cx| {
                    if window.is_window_active() {
                        this.refresh_configuration();
                        this.reload_preview(cx);
                    }
                },
            ),
        };
        panel.load_filters(cx);
        panel.reload_preview(cx);
        panel
    }

    pub fn reload_configuration(&mut self, cx: &mut Context<Self>) {
        self.refresh_configuration();
        cx.notify();
    }

    fn load_runtime_config() -> AiRuntimeConfig {
        let settings = load_app_settings()
            .map(|app_settings| app_settings.ai)
            .unwrap_or_default();
        let has_api_key = load_secret(settings.protocol.secret_account())
            .ok()
            .flatten()
            .is_some();
        AiRuntimeConfig {
            settings,
            has_api_key,
        }
    }

    fn refresh_configuration(&mut self) {
        self.config = Self::load_runtime_config();
    }

    fn load_filters(&mut self, cx: &mut Context<Self>) {
        let tag_store = self.store.clone();
        cx.spawn(async move |view, cx| {
            if let Ok(tags) = tag_store.get_all_tags().await {
                let _ = view.update(cx, |panel, cx| {
                    panel.available_tags = tags.into_iter().map(|tag| tag.name).collect();
                    cx.notify();
                });
            }
        })
        .detach();

        let person_store = self.store.clone();
        cx.spawn(async move |view, cx| {
            if let Ok(persons) = person_store.get_all_persons().await {
                let _ = view.update(cx, |panel, cx| {
                    panel.available_persons =
                        persons.into_iter().map(|person| person.name).collect();
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn current_date_range(&self, cx: &App) -> Result<(NaiveDate, NaiveDate), String> {
        let start = self
            .start_date_picker
            .read(cx)
            .date()
            .start()
            .ok_or_else(|| "请选择开始日期".to_string())?;
        let end = self
            .end_date_picker
            .read(cx)
            .date()
            .start()
            .ok_or_else(|| "请选择结束日期".to_string())?;
        Ok((start, end))
    }

    fn build_query(&self, cx: &App) -> Result<AiContextQuery, String> {
        let (start_date, end_date) = self.current_date_range(cx)?;
        let (start_at, end_at) = local_day_range_to_utc(start_date, end_date)?;
        Ok(AiContextQuery {
            start_at,
            end_at,
            tags: self.selected_tags.iter().cloned().collect(),
            persons: self.selected_persons.iter().cloned().collect(),
            mode: self.mode,
        })
    }

    fn reload_preview(&mut self, cx: &mut Context<Self>) {
        let query = match self.build_query(cx) {
            Ok(query) => query,
            Err(err) => {
                self.preview = None;
                self.error = Some(err);
                self.notice = None;
                cx.notify();
                return;
            }
        };

        self.request_serial = self.request_serial.wrapping_add(1);
        let request_serial = self.request_serial;
        self.preview_loading = true;
        self.error = None;
        self.notice = None;
        let store = self.store.clone();

        cx.spawn(async move |view, cx| {
            let records = store.get_all_records().await;
            let _ = view.update(cx, |panel, cx| {
                if panel.request_serial != request_serial {
                    return;
                }

                panel.preview_loading = false;
                match records {
                    Ok(records) => {
                        panel.preview = Some(build_context_bundle(&records, &query));
                        panel.error = None;
                    }
                    Err(err) => {
                        panel.preview = None;
                        panel.error = Some(format!("加载 AI 上下文失败: {err}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn set_mode(&mut self, mode: AiSummaryMode, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.result = None;
        self.reload_preview(cx);
    }

    fn toggle_tag(&mut self, tag: &str, cx: &mut Context<Self>) {
        if !self.selected_tags.remove(tag) {
            self.selected_tags.insert(tag.to_string());
        }
        self.result = None;
        self.reload_preview(cx);
    }

    fn toggle_person(&mut self, person: &str, cx: &mut Context<Self>) {
        if !self.selected_persons.remove(person) {
            self.selected_persons.insert(person.to_string());
        }
        self.result = None;
        self.reload_preview(cx);
    }

    fn clear_tags(&mut self, cx: &mut Context<Self>) {
        if self.selected_tags.is_empty() {
            return;
        }
        self.selected_tags.clear();
        self.result = None;
        self.reload_preview(cx);
    }

    fn clear_persons(&mut self, cx: &mut Context<Self>) {
        if self.selected_persons.is_empty() {
            return;
        }
        self.selected_persons.clear();
        self.result = None;
        self.reload_preview(cx);
    }

    fn generate(&mut self, cx: &mut Context<Self>) {
        let query = match self.build_query(cx) {
            Ok(query) => query,
            Err(err) => {
                self.error = Some(err);
                self.notice = None;
                cx.notify();
                return;
            }
        };

        let Some(bundle) = self.preview.clone() else {
            self.error = Some("当前没有可生成的上下文，请先调整筛选范围".to_string());
            self.notice = None;
            cx.notify();
            return;
        };

        if bundle.records.is_empty() {
            self.error = Some("当前筛选范围内没有命中记录，无法生成总结".to_string());
            self.notice = None;
            cx.notify();
            return;
        }

        self.generating = true;
        self.error = None;
        self.notice = None;
        self.result = None;
        self.refresh_configuration();

        let settings = self.config.settings.clone();
        let api_key = match load_secret(settings.protocol.secret_account()) {
            Ok(Some(api_key)) => api_key,
            Ok(None) => {
                self.generating = false;
                self.error = Some("当前协议还没有保存 API Key，请先去设置里配置".to_string());
                cx.notify();
                return;
            }
            Err(err) => {
                self.generating = false;
                self.error = Some(err);
                cx.notify();
                return;
            }
        };

        cx.spawn(async move |view, cx| {
            let result = generate_summary(&settings, &api_key, &query, &bundle).await;
            let _ = view.update(cx, |panel, cx| {
                panel.generating = false;
                match result {
                    Ok(text) => {
                        panel.result = Some(text);
                        panel.notice = Some("AI 总结已生成，可以复制结果".to_string());
                        panel.error = None;
                    }
                    Err(err) => {
                        panel.result = None;
                        panel.notice = None;
                        panel.error = Some(err);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn copy_result(&mut self, cx: &mut Context<Self>) {
        let Some(result) = self.result.clone() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(result));
        self.notice = Some("已复制 AI 总结".to_string());
        self.error = None;
        cx.notify();
    }

    fn render_mode_button(
        &self,
        mode: AiSummaryMode,
        id: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        Button::new(id)
            .child(mode.label())
            .when(self.mode == mode, |button| {
                button.with_variant(gpui_component::button::ButtonVariant::Primary)
            })
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.set_mode(mode, cx);
            }))
            .into_any_element()
    }

    fn render_filter_chip<F>(
        &self,
        id: impl Into<String>,
        label: &str,
        selected: bool,
        on_click: F,
    ) -> AnyElement
    where
        F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    {
        Button::new(id.into())
            .child(label.to_string())
            .when(selected, |button| {
                button.with_variant(gpui_component::button::ButtonVariant::Primary)
            })
            .small()
            .on_click(on_click)
            .into_any_element()
    }

    fn render_message(&self) -> Option<AnyElement> {
        if let Some(message) = self.notice.as_deref() {
            return Some(render_message_box(message, false));
        }

        self.error
            .as_deref()
            .map(|message| render_message_box(message, true))
    }

    fn render_preview_card(&self) -> AnyElement {
        let Some(preview) = self.preview.as_ref() else {
            return render_card(
                "上下文预览",
                "根据当前筛选范围预估将发送给 AI 的上下文。",
                vec![div()
                    .text_sm()
                    .text_color(rgb(0x8c8c8c))
                    .child("还没有可用的上下文。")
                    .into_any_element()],
            );
        };

        render_card(
            "上下文预览",
            "先由本地筛选和压缩，再交给模型生成结果，不直接让模型反查数据库。",
            vec![
                render_info_line("时间范围", &preview.date_span_label).into_any_element(),
                render_info_line("命中记录", &format!("{} 条", preview.records.len()))
                    .into_any_element(),
                render_info_line(
                    "任务构成",
                    &format!(
                        "{} 条任务，其中 {} 条未完成，{} 条已完成",
                        preview.task_count, preview.open_task_count, preview.completed_task_count
                    ),
                )
                .into_any_element(),
                render_info_line("记录/想法/事件", &format!("{} 条", preview.note_like_count))
                    .into_any_element(),
                render_info_line("分层摘要批次", &format!("{} 批", preview.chunk_count))
                    .into_any_element(),
                render_info_line(
                    "估算上下文长度",
                    &format!("约 {} 字符", preview.estimated_chars),
                )
                .into_any_element(),
            ],
        )
    }
}

impl Render for AiPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let config_ready = self.config.settings.has_connection_config() && self.config.has_api_key;
        let two_column = window.viewport_size().width >= AI_TWO_COLUMN_BREAKPOINT;

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .size_full()
                    .overflow_y_scrollbar()
                    .p(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex()
                            .min_w(px(0.0))
                            .when(two_column, |this| {
                                this.items_start().justify_between()
                            })
                            .when(!two_column, |this| this.flex_col().items_start())
                            .flex_col()
                            .gap(px(16.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.0))
                                    .min_w(px(0.0))
                                    .max_w(px(760.0))
                                    .child(
                                        div()
                                            .text_xl()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(0x262626))
                                            .child("AI 总结"),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(0x8c8c8c))
                                            .line_height(relative(1.5))
                                            .child("先选范围，再生成过去总结或未来提炼。首版不做通用聊天，只做可重复的结构化输出。"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .min_w(px(0.0))
                                    .when(two_column, |this| this.items_end())
                                    .when(!two_column, |this| this.items_start())
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(if config_ready {
                                                rgb(0x389e0d)
                                            } else {
                                                rgb(0xcf1322)
                                            })
                                            .child(if config_ready {
                                                "AI 连接已就绪"
                                            } else {
                                                "AI 连接未完成"
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(0x8c8c8c))
                                            .line_height(relative(1.5))
                                            .child(format!(
                                                "{} · {}",
                                                self.config.settings.protocol.label(),
                                                if self.config.settings.model.trim().is_empty() {
                                                    "未设置模型"
                                                } else {
                                                    self.config.settings.model.trim()
                                                }
                                            )),
                                    ),
                            ),
                    )
                    .when_some(self.render_message(), |this, message| this.child(message))
                    .child(
                        div()
                            .flex()
                            .gap(px(16.0))
                            .when(two_column, |this| this.items_start())
                            .when(!two_column, |this| this.flex_col())
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(16.0))
                                    .min_w(px(0.0))
                                    .w_full()
                                    .when(two_column, |this| this.max_w(px(520.0)))
                                    .child(render_card(
                                        "生成模式",
                                        "过去总结适合复盘，未来提炼会额外纳入当前未完成任务。",
                                        vec![
                                            div()
                                                .flex()
                                                .flex_wrap()
                                                .gap(px(10.0))
                                                .child(
                                                    self.render_mode_button(
                                                        AiSummaryMode::PastSummary,
                                                        "ai-mode-past",
                                                        cx,
                                                    ),
                                                )
                                                .child(
                                                    self.render_mode_button(
                                                        AiSummaryMode::FutureTasks,
                                                        "ai-mode-future",
                                                        cx,
                                                    ),
                                                )
                                                .into_any_element(),
                                        ],
                                    ))
                                    .child(render_card(
                                        "时间范围",
                                        "时间范围由本地先做确定性筛选。",
                                        vec![
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(8.0))
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(rgb(0x595959))
                                                        .child("开始日期"),
                                                )
                                                .child(DatePicker::new(&self.start_date_picker))
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(rgb(0x595959))
                                                        .child("结束日期"),
                                                )
                                                .child(DatePicker::new(&self.end_date_picker))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_wrap()
                                                        .gap(px(10.0))
                                                        .child(
                                                            Button::new("ai-refresh-preview")
                                                                .child(if self.preview_loading {
                                                                    "刷新中..."
                                                                } else {
                                                                    "刷新预览"
                                                                })
                                                                .when(self.preview_loading, |button| {
                                                                    button.disabled(true)
                                                                })
                                                                .on_click(cx.listener(
                                                                    |this, _event, _window, cx| {
                                                                        this.result = None;
                                                                        this.reload_preview(cx);
                                                                    },
                                                                )),
                                                        )
                                                        .child(
                                                            Button::new("ai-generate")
                                                                .child(if self.generating {
                                                                    "生成中..."
                                                                } else {
                                                                    "生成总结"
                                                                })
                                                                .when(!self.generating, |button| {
                                                                    button.with_variant(
                                                                        gpui_component::button::ButtonVariant::Primary,
                                                                    )
                                                                })
                                                                .disabled(self.generating)
                                                                .on_click(cx.listener(
                                                                    |this, _event, _window, cx| {
                                                                        this.generate(cx);
                                                                    },
                                                                )),
                                                        )
                                                        .into_any_element(),
                                                )
                                                .into_any_element(),
                                        ],
                                    ))
                                    .child(render_card(
                                        "标签筛选",
                                        "多选时按“同时命中”处理。",
                                        vec![
                                            div()
                                                .flex()
                                                .flex_wrap()
                                                .gap(px(8.0))
                                                .children(self.available_tags.iter().enumerate().map(
                                                    |(idx, tag)| {
                                                        self.render_filter_chip(
                                                            format!("ai-tag-{idx}"),
                                                            tag,
                                                            self.selected_tags.contains(tag),
                                                            cx.listener({
                                                                let tag = tag.clone();
                                                                move |this, _event, _window, cx| {
                                                                    this.toggle_tag(&tag, cx);
                                                                }
                                                            }),
                                                        )
                                                    },
                                                ))
                                                .into_any_element(),
                                            Button::new("ai-clear-tags")
                                                .child("清除标签筛选")
                                                .on_click(cx.listener(|this, _event, _window, cx| {
                                                    this.clear_tags(cx);
                                                }))
                                                .into_any_element(),
                                        ],
                                    ))
                                    .child(render_card(
                                        "人物筛选",
                                        "多选时同样按“同时命中”处理。",
                                        vec![
                                            div()
                                                .flex()
                                                .flex_wrap()
                                                .gap(px(8.0))
                                                .children(self.available_persons.iter().enumerate().map(
                                                    |(idx, person)| {
                                                        self.render_filter_chip(
                                                            format!("ai-person-{idx}"),
                                                            person,
                                                            self.selected_persons.contains(person),
                                                            cx.listener({
                                                                let person = person.clone();
                                                                move |this, _event, _window, cx| {
                                                                    this.toggle_person(&person, cx);
                                                                }
                                                            }),
                                                        )
                                                    },
                                                ))
                                                .into_any_element(),
                                            Button::new("ai-clear-persons")
                                                .child("清除人物筛选")
                                                .on_click(cx.listener(|this, _event, _window, cx| {
                                                    this.clear_persons(cx);
                                                }))
                                                .into_any_element(),
                                        ],
                                    )),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .w_full()
                                    .flex()
                                    .flex_col()
                                    .gap(px(16.0))
                                    .child(self.render_preview_card())
                                    .child(render_card(
                                        "结果",
                                        "生成完成后可直接复制。",
                                        vec![
                                            div()
                                                .flex()
                                                .gap(px(16.0))
                                                .when(two_column, |this| {
                                                    this.items_center().justify_between()
                                                })
                                                .when(!two_column, |this| this.flex_col())
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(rgb(0x8c8c8c))
                                                        .line_height(relative(1.5))
                                                        .child(if self.generating {
                                                            "正在分层压缩上下文并请求模型…"
                                                        } else if self.result.is_some() {
                                                            "最近一次结果已生成。"
                                                        } else {
                                                            "还没有生成结果。"
                                                        }),
                                                )
                                                .child(
                                                    Button::new("ai-copy-result")
                                                        .child("复制结果")
                                                        .disabled(self.result.is_none())
                                                        .on_click(cx.listener(|this, _event, _window, cx| {
                                                            this.copy_result(cx);
                                                        })),
                                                )
                                                .into_any_element(),
                                            div()
                                                .w_full()
                                                .min_h(px(320.0))
                                                .max_h(px(720.0))
                                                .overflow_y_scrollbar()
                                                .p(px(14.0))
                                                .rounded(px(12.0))
                                                .bg(rgb(0xfcfcfc))
                                                .border_1()
                                                .border_color(rgb(0xf0f0f0))
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .line_height(relative(1.6))
                                                        .text_color(rgb(0x262626))
                                                        .child(
                                                            self.result.clone().unwrap_or_else(|| {
                                                                "选择范围后点击“生成总结”，这里会显示结构化结果。"
                                                                    .to_string()
                                                            }),
                                                        ),
                                                )
                                                .into_any_element(),
                                        ],
                                    )),
                            ),
                    ),
            )
    }
}

fn render_card(
    title: &'static str,
    description: &'static str,
    children: Vec<AnyElement>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .p(px(16.0))
        .rounded(px(14.0))
        .border_1()
        .border_color(rgb(0xf0f0f0))
        .bg(rgb(0xffffff))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x262626))
                        .child(title),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x8c8c8c))
                        .line_height(relative(1.5))
                        .child(description),
                ),
        )
        .children(children)
        .into_any_element()
}

fn render_info_line(label: &'static str, value: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(0x595959))
                .child(label),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(0x262626))
                .line_height(relative(1.5))
                .child(value.to_string()),
        )
}

fn render_message_box(message: &str, is_error: bool) -> AnyElement {
    let (background, border, text) = if is_error {
        (rgb(0xfff2f0), rgb(0xffccc7), rgb(0xcf1322))
    } else {
        (rgb(0xf6ffed), rgb(0xb7eb8f), rgb(0x389e0d))
    };

    div()
        .p(px(12.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(border)
        .bg(background)
        .child(
            div()
                .text_sm()
                .text_color(text)
                .line_height(relative(1.5))
                .child(message.to_string()),
        )
        .into_any_element()
}
