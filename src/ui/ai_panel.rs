use crate::ai::{
    build_context_bundle, generate_summary, local_day_range_to_utc, AiContextBundle,
    AiContextQuery, AiSettings, AiSummaryMode,
};
use crate::platform::load_secret;
use crate::settings::load_app_settings;
use crate::store::Store;
use chrono::{Datelike, Duration, Local, NaiveDate};
use gpui::{prelude::*, ClipboardItem, *};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::date_picker::{DatePicker, DatePickerState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::{Disableable, Sizable};
use std::collections::BTreeSet;

const AI_TWO_COLUMN_BREAKPOINT: Pixels = px(1280.0);
const AI_EVIDENCE_SAMPLE_LIMIT: usize = 6;

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
        let has_api_key = load_secret(settings.protocol.api_key_env_var())
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
        let api_key = match load_secret(settings.protocol.api_key_env_var()) {
            Ok(Some(api_key)) => api_key,
            Ok(None) => {
                self.generating = false;
                self.error = Some(format!(
                    "当前协议还没有检测到环境变量 {}，请先去设置里配置",
                    settings.protocol.api_key_env_var()
                ));
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

    fn set_quick_range(&mut self, days: i64, window: &mut Window, cx: &mut Context<Self>) {
        let end = Local::now().date_naive();
        let start = end - Duration::days(days.saturating_sub(1));
        self.start_date_picker.update(cx, |picker, cx| {
            picker.set_date(start, window, cx);
        });
        self.end_date_picker.update(cx, |picker, cx| {
            picker.set_date(end, window, cx);
        });
        self.result = None;
        self.reload_preview(cx);
    }

    fn set_current_week_range(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let end = Local::now().date_naive();
        let start = end - Duration::days(end.weekday().num_days_from_monday() as i64);
        self.start_date_picker.update(cx, |picker, cx| {
            picker.set_date(start, window, cx);
        });
        self.end_date_picker.update(cx, |picker, cx| {
            picker.set_date(end, window, cx);
        });
        self.result = None;
        self.reload_preview(cx);
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

    fn render_analysis_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        render_card(
            "分析控制",
            "先定模式和时间，再生成结果。",
            vec![
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(self.render_mode_button(AiSummaryMode::PastSummary, "ai-mode-past", cx))
                    .child(self.render_mode_button(
                        AiSummaryMode::FutureTasks,
                        "ai-mode-future",
                        cx,
                    ))
                    .child(
                        Button::new("ai-generate")
                            .child(if self.generating {
                                "生成中..."
                            } else {
                                "生成总结"
                            })
                            .when(!self.generating, |button| {
                                button.with_variant(gpui_component::button::ButtonVariant::Primary)
                            })
                            .disabled(self.generating)
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.generate(cx);
                            })),
                    )
                    .into_any_element(),
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(self.render_filter_chip(
                        "ai-range-7d",
                        "最近 7 天",
                        false,
                        cx.listener(|this, _event, window, cx| {
                            this.set_quick_range(7, window, cx);
                        }),
                    ))
                    .child(self.render_filter_chip(
                        "ai-range-week",
                        "本周",
                        false,
                        cx.listener(|this, _event, window, cx| {
                            this.set_current_week_range(window, cx);
                        }),
                    ))
                    .child(self.render_filter_chip(
                        "ai-range-30d",
                        "最近 30 天",
                        false,
                        cx.listener(|this, _event, window, cx| {
                            this.set_quick_range(30, window, cx);
                        }),
                    ))
                    .into_any_element(),
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(10.0))
                    .items_end()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .min_w(px(180.0))
                            .child(div().text_xs().text_color(rgb(0x595959)).child("开始日期"))
                            .child(DatePicker::new(&self.start_date_picker)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .min_w(px(180.0))
                            .child(div().text_xs().text_color(rgb(0x595959)).child("结束日期"))
                            .child(DatePicker::new(&self.end_date_picker)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(8.0))
                            .child(
                                Button::new("ai-refresh-preview")
                                    .child(if self.preview_loading {
                                        "刷新中..."
                                    } else {
                                        "刷新预览"
                                    })
                                    .when(self.preview_loading, |button| button.disabled(true))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.result = None;
                                        this.reload_preview(cx);
                                    })),
                            )
                            .into_any_element(),
                    )
                    .into_any_element(),
            ],
        )
    }

    fn render_filter_section(
        &self,
        title: &'static str,
        values: &[String],
        selected: &BTreeSet<String>,
        empty_hint: &'static str,
        clear_label: &'static str,
        cx: &mut Context<Self>,
        is_tag: bool,
    ) -> AnyElement {
        let chips = if values.is_empty() {
            div()
                .text_sm()
                .text_color(rgb(0x8c8c8c))
                .child(empty_hint)
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_wrap()
                .gap(px(8.0))
                .children(values.iter().enumerate().map(|(idx, value)| {
                    if is_tag {
                        self.render_filter_chip(
                            format!("ai-tag-{idx}"),
                            value,
                            selected.contains(value),
                            cx.listener({
                                let value = value.clone();
                                move |this, _event, _window, cx| {
                                    this.toggle_tag(&value, cx);
                                }
                            }),
                        )
                    } else {
                        self.render_filter_chip(
                            format!("ai-person-{idx}"),
                            value,
                            selected.contains(value),
                            cx.listener({
                                let value = value.clone();
                                move |this, _event, _window, cx| {
                                    this.toggle_person(&value, cx);
                                }
                            }),
                        )
                    }
                }))
                .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0x595959))
                            .child(title),
                    )
                    .child(div().text_xs().text_color(rgb(0x8c8c8c)).child(
                        if selected.is_empty() {
                            "未筛选"
                        } else {
                            "已筛选"
                        },
                    ))
                    .when(!selected.is_empty(), |this| {
                        this.child(
                            Button::new(format!("ai-clear-{title}"))
                                .child(clear_label)
                                .small()
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    if is_tag {
                                        this.clear_tags(cx);
                                    } else {
                                        this.clear_persons(cx);
                                    }
                                })),
                        )
                    }),
            )
            .child(chips)
            .into_any_element()
    }

    fn render_scope_filters_card(&self, cx: &mut Context<Self>) -> AnyElement {
        render_card(
            "收窄范围",
            "标签和人物是次级过滤器，只在你想进一步收窄分析对象时使用。",
            vec![
                self.render_filter_section(
                    "标签",
                    &self.available_tags,
                    &self.selected_tags,
                    "当前未建立标签，先按时间范围分析。",
                    "清除标签",
                    cx,
                    true,
                ),
                self.render_filter_section(
                    "人物",
                    &self.available_persons,
                    &self.selected_persons,
                    "当前未建立人物关联，先按时间范围分析。",
                    "清除人物",
                    cx,
                    false,
                ),
            ],
        )
    }

    fn render_result_canvas(&self, cx: &mut Context<Self>) -> AnyElement {
        let header =
            div()
                .flex()
                .gap(px(12.0))
                .when(self.result.is_some(), |this| {
                    this.items_center().justify_between()
                })
                .when(self.result.is_none(), |this| this.flex_col())
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(0x262626))
                                .child("结果画布"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0x8c8c8c))
                                .line_height(relative(1.5))
                                .child(if self.generating {
                                    "正在整理上下文、分层压缩并请求模型。"
                                } else if self.result.is_some() {
                                    "结果已经生成，可以继续阅读、复制或调整范围后重跑。"
                                } else {
                                    "这里展示本次分析的最终结果。先设定范围，再点击生成。"
                                }),
                        ),
                )
                .when(self.result.is_some(), |this| {
                    this.child(Button::new("ai-copy-result").child("复制结果").on_click(
                        cx.listener(|this, _event, _window, cx| {
                            this.copy_result(cx);
                        }),
                    ))
                });

        let body = if self.generating {
            div()
                .min_h(px(360.0))
                .flex()
                .flex_col()
                .justify_center()
                .gap(px(12.0))
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0x262626))
                        .child("正在生成 AI 总结…"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x8c8c8c))
                        .line_height(relative(1.6))
                        .child("这一步会先根据当前筛选范围构建上下文，再按需要分批压缩后生成最终结果。"),
                )
                .into_any_element()
        } else if let Some(result) = self.result.as_ref() {
            div()
                .min_h(px(360.0))
                .max_h(px(760.0))
                .overflow_y_scrollbar()
                .p(px(18.0))
                .rounded(px(14.0))
                .bg(rgb(0xfcfcfc))
                .border_1()
                .border_color(rgb(0xf0f0f0))
                .child(
                    div()
                        .text_sm()
                        .line_height(relative(1.7))
                        .text_color(rgb(0x262626))
                        .child(result.clone()),
                )
                .into_any_element()
        } else {
            let preview = self.preview.as_ref();
            div()
                .min_h(px(360.0))
                .p(px(22.0))
                .rounded(px(14.0))
                .bg(rgb(0xfcfcfc))
                .border_1()
                .border_color(rgb(0xf0f0f0))
                .flex()
                .flex_col()
                .justify_center()
                .gap(px(14.0))
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0x262626))
                        .child("选择一个范围开始分析"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x8c8c8c))
                        .line_height(relative(1.6))
                        .child(
                            preview.map_or_else(
                                || "你可以直接用“最近 7 天”或“本周”，再按标签、人物收窄范围。".to_string(),
                                |preview| {
                                    format!(
                                        "当前范围已命中 {} 条记录，其中 {} 条任务、{} 条记录/想法/事件。准备好后点击“生成总结”。",
                                        preview.records.len(),
                                        preview.task_count,
                                        preview.note_like_count
                                    )
                                },
                            ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(10.0))
                        .child(
                            self.render_filter_chip(
                                "ai-empty-7d",
                                "最近 7 天",
                                false,
                                cx.listener(|this, _event, window, cx| {
                                    this.set_quick_range(7, window, cx);
                                }),
                            ),
                        )
                        .child(
                            self.render_filter_chip(
                                "ai-empty-week",
                                "本周",
                                false,
                                cx.listener(|this, _event, window, cx| {
                                    this.set_current_week_range(window, cx);
                                }),
                            ),
                        )
                        .child(
                            self.render_filter_chip(
                                "ai-empty-30d",
                                "最近 30 天",
                                false,
                                cx.listener(|this, _event, window, cx| {
                                    this.set_quick_range(30, window, cx);
                                }),
                            ),
                        )
                        .into_any_element(),
                )
                .into_any_element()
        };

        render_card(
            "分析结果",
            "结果区是主画布，整个面板围绕这次分析展开。",
            vec![header.into_any_element(), body],
        )
    }

    fn render_evidence_panel(&self) -> AnyElement {
        let metrics = if let Some(preview) = self.preview.as_ref() {
            div()
                .flex()
                .flex_wrap()
                .gap(px(10.0))
                .child(render_metric_tile("时间范围", &preview.date_span_label))
                .child(render_metric_tile(
                    "命中记录",
                    &format!("{} 条", preview.records.len()),
                ))
                .child(render_metric_tile(
                    "任务",
                    &format!(
                        "{} 条 / 未完成 {} 条",
                        preview.task_count, preview.open_task_count
                    ),
                ))
                .child(render_metric_tile(
                    "记录/想法",
                    &format!("{} 条", preview.note_like_count),
                ))
                .child(render_metric_tile(
                    "摘要批次",
                    &format!("{} 批", preview.chunk_count),
                ))
                .child(render_metric_tile(
                    "上下文估算",
                    &format!("约 {} 字符", preview.estimated_chars),
                ))
                .into_any_element()
        } else {
            div()
                .text_sm()
                .text_color(rgb(0x8c8c8c))
                .child("当前还没有可展示的分析证据。")
                .into_any_element()
        };

        let samples = if let Some(preview) = self.preview.as_ref() {
            if preview.records.is_empty() {
                div()
                    .text_sm()
                    .text_color(rgb(0x8c8c8c))
                    .child("当前筛选范围没有命中记录。")
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .children(
                        preview
                            .records
                            .iter()
                            .take(AI_EVIDENCE_SAMPLE_LIMIT)
                            .map(render_record_sample),
                    )
                    .into_any_element()
            }
        } else {
            div()
                .text_sm()
                .text_color(rgb(0x8c8c8c))
                .child("等待本地预览完成后，会在这里显示 AI 这次会看到的证据样本。")
                .into_any_element()
        };

        render_card(
            "证据栏",
            "这里解释 AI 这次会看什么，以及为什么会得到当前结果。",
            vec![
                metrics,
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0x595959))
                            .child("命中记录样本"),
                    )
                    .child(samples)
                    .into_any_element(),
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0x595959))
                            .child("处理说明"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x8c8c8c))
                            .line_height(relative(1.6))
                            .child("本次分析先由本地做确定性筛选；如果上下文过大，会先分批压缩，再合并成最终结果，不让模型自由反查数据库。"),
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
        let status_badge = if config_ready {
            "AI 连接已就绪"
        } else {
            "AI 连接未完成"
        };
        let status_detail = if !self.config.settings.has_connection_config() {
            "请先在设置里保存 Base URL 和 Model".to_string()
        } else if !self.config.has_api_key {
            format!(
                "当前协议还没有检测到 {}",
                self.config.settings.protocol.api_key_env_var()
            )
        } else {
            format!(
                "{} · {}",
                self.config.settings.protocol.label(),
                if self.config.settings.model.trim().is_empty() {
                    "未设置模型"
                } else {
                    self.config.settings.model.trim()
                }
            )
        };

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
                            .gap(px(16.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.0))
                                    .min_w(px(0.0))
                                    .max_w(px(780.0))
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
                                    .gap(px(6.0))
                                    .min_w(px(0.0))
                                    .when(two_column, |this| this.items_end())
                                    .when(!two_column, |this| this.items_start())
                                    .child(
                                        div()
                                            .text_xs()
                                            .px(px(10.0))
                                            .py(px(6.0))
                                            .rounded(px(999.0))
                                            .bg(if config_ready {
                                                rgb(0xf6ffed)
                                            } else {
                                                rgb(0xfff2f0)
                                            })
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(if config_ready {
                                                rgb(0x389e0d)
                                            } else {
                                                rgb(0xcf1322)
                                            })
                                            .child(status_badge),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(0x8c8c8c))
                                            .line_height(relative(1.5))
                                            .child(status_detail),
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
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .w_full()
                                    .flex()
                                    .flex_col()
                                    .gap(px(16.0))
                                    .child(self.render_result_canvas(cx))
                                    .child(self.render_evidence_panel()),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .min_w(px(0.0))
                                    .when(two_column, |this| this.max_w(px(360.0)))
                                    .flex()
                                    .flex_col()
                                    .gap(px(16.0))
                                    .child(self.render_analysis_bar(cx))
                                    .child(self.render_scope_filters_card(cx)),
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

fn render_metric_tile(label: &'static str, value: &str) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .min_w(px(140.0))
        .p(px(12.0))
        .rounded(px(12.0))
        .bg(rgb(0xfcfcfc))
        .border_1()
        .border_color(rgb(0xf0f0f0))
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(0x8c8c8c))
                .child(label),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(0x262626))
                .line_height(relative(1.5))
                .child(value.to_string()),
        )
        .into_any_element()
}

fn render_record_sample(record: &crate::models::Record) -> AnyElement {
    let kind = match record.record_type {
        crate::models::RecordType::Task => "任务",
        crate::models::RecordType::Note => "记录",
        crate::models::RecordType::Idea => "想法",
        crate::models::RecordType::Event => "事件",
    };
    let body = truncate_preview(&record.content, 88);

    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .p(px(12.0))
        .rounded(px(12.0))
        .bg(rgb(0xfcfcfc))
        .border_1()
        .border_color(rgb(0xf0f0f0))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(8.0))
                .items_center()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0x8c8c8c))
                        .child(kind),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0x262626))
                        .child(record.display_title()),
                ),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(0x8c8c8c))
                .line_height(relative(1.5))
                .child(body),
        )
        .into_any_element()
}

fn truncate_preview(input: &str, limit: usize) -> String {
    let trimmed = input.replace('\n', " ");
    let mut chars = trimmed.chars();
    let preview = chars.by_ref().take(limit).collect::<String>();
    if chars.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
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
