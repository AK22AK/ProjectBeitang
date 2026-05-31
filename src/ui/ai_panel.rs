use crate::ai::{
    build_context_bundle, generate_summary, local_day_range_to_utc, AiContextBundle,
    AiContextQuery, AiRecordScope, AiSettings, AiSummaryMode,
};
use crate::platform::{
    load_latest_ai_usage, load_secret, load_today_ai_usage, record_ai_usage, AiDailyUsageEntry,
    AiLatestUsageSnapshot, AiUsageEventKind, SecretSource,
};
use crate::settings::load_app_settings;
use crate::store::Store;
use chrono::{Datelike, Duration, Local, NaiveDate};
use gpui::{prelude::*, ClipboardItem, *};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::date_picker::{DatePicker, DatePickerState};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::{v_flex, Disableable, IconName, Sizable};
use std::collections::BTreeSet;

#[derive(Clone)]
struct AiRuntimeConfig {
    settings: AiSettings,
    has_api_key: bool,
    source: Option<SecretSource>,
}

#[derive(Default)]
struct MetadataFilters {
    available_tags: Vec<String>,
    available_persons: Vec<String>,
    selected_tags: BTreeSet<String>,
    selected_persons: BTreeSet<String>,
}

impl MetadataFilters {
    fn replace_catalog(&mut self, tags: Vec<String>, persons: Vec<String>) {
        self.available_tags = tags;
        self.available_persons = persons;
        self.selected_tags
            .retain(|tag| self.available_tags.iter().any(|candidate| candidate == tag));
        self.selected_persons.retain(|person| {
            self.available_persons
                .iter()
                .any(|candidate| candidate == person)
        });
    }

    fn build_query_values(&self) -> (Vec<String>, Vec<String>) {
        (
            self.selected_tags.iter().cloned().collect(),
            self.selected_persons.iter().cloned().collect(),
        )
    }

    fn toggle_tag(&mut self, tag: &str) {
        if !self.selected_tags.remove(tag) {
            self.selected_tags.insert(tag.to_string());
        }
    }

    fn toggle_person(&mut self, person: &str) {
        if !self.selected_persons.remove(person) {
            self.selected_persons.insert(person.to_string());
        }
    }
}

#[derive(Clone, Copy)]
enum MetadataFilterKind {
    Tag,
    Person,
}

pub struct AiPanel {
    store: Store,
    mode: AiSummaryMode,
    record_scope: AiRecordScope,
    start_date_picker: Entity<DatePickerState>,
    end_date_picker: Entity<DatePickerState>,
    keyword_input: Entity<InputState>,
    custom_request_input: Entity<InputState>,
    filters: MetadataFilters,
    preview: Option<AiContextBundle>,
    config: AiRuntimeConfig,
    preview_loading: bool,
    generating: bool,
    samples_expanded: bool,
    today_usage: Option<AiDailyUsageEntry>,
    last_request_usage: Option<AiLatestUsageSnapshot>,
    result: Option<String>,
    notice: Option<String>,
    error: Option<String>,
    request_serial: usize,
    _subscriptions: Vec<Subscription>,
    _window_activation_subscription: Subscription,
}

impl AiPanel {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let today = Local::now().date_naive();
        let start = today - Duration::days(6);
        let keyword_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("关键字过滤标题和正文"));
        let custom_request_input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .auto_grow(2, 4)
                .placeholder("补充你的要求，例如：按项目拆分，指出阻塞和下周优先级")
        });
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
        let custom_request_template = AiSummaryMode::PastSummary.default_request_template();

        custom_request_input.update(cx, |input, cx| {
            input.set_value(custom_request_template, window, cx);
        });

        let subscriptions = vec![
            cx.subscribe_in(
                &keyword_input,
                window,
                |this, input, event: &InputEvent, _window, cx| {
                    if let InputEvent::Change = event {
                        let _ = input.read(cx).text();
                        this.result = None;
                        this.reload_preview(cx);
                    }
                },
            ),
            cx.subscribe_in(
                &custom_request_input,
                window,
                |this, _input, event: &InputEvent, _window, cx| {
                    if let InputEvent::Change = event {
                        this.result = None;
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &start_date_picker,
                window,
                |this, _, _: &gpui_component::date_picker::DatePickerEvent, _window, cx| {
                    this.result = None;
                    this.reload_preview(cx);
                },
            ),
            cx.subscribe_in(
                &end_date_picker,
                window,
                |this, _, _: &gpui_component::date_picker::DatePickerEvent, _window, cx| {
                    this.result = None;
                    this.reload_preview(cx);
                },
            ),
        ];

        let mut panel = Self {
            store,
            mode: AiSummaryMode::PastSummary,
            record_scope: AiRecordScope::All,
            start_date_picker,
            end_date_picker,
            keyword_input,
            custom_request_input,
            filters: MetadataFilters::default(),
            preview: None,
            config: Self::load_runtime_config(),
            preview_loading: false,
            generating: false,
            samples_expanded: false,
            today_usage: None,
            last_request_usage: None,
            result: None,
            notice: None,
            error: None,
            request_serial: 0,
            _subscriptions: subscriptions,
            _window_activation_subscription: cx.observe_window_activation(
                window,
                |this, window, cx| {
                    if window.is_window_active() {
                        this.refresh_configuration();
                        this.refresh_usage_metrics();
                        this.reload_filters(cx);
                        this.reload_preview(cx);
                    }
                },
            ),
        };
        panel.refresh_usage_metrics();
        panel.reload_filters(cx);
        panel.reload_preview(cx);
        panel
    }

    pub fn reload_configuration(&mut self, cx: &mut Context<Self>) {
        self.refresh_configuration();
        self.refresh_usage_metrics();
        self.reload_filters(cx);
        cx.notify();
    }

    fn load_runtime_config() -> AiRuntimeConfig {
        let settings = load_app_settings()
            .map(|app_settings| app_settings.ai)
            .unwrap_or_default();
        let (has_api_key, source) = match load_secret(settings.protocol) {
            Ok(Some(secret)) => (true, Some(secret.source)),
            Ok(None) | Err(_) => (false, None),
        };
        AiRuntimeConfig {
            settings,
            has_api_key,
            source,
        }
    }

    fn refresh_configuration(&mut self) {
        self.config = Self::load_runtime_config();
    }

    fn refresh_usage_metrics(&mut self) {
        let protocol = self.config.settings.protocol;
        self.today_usage = load_today_ai_usage(protocol).ok().flatten();
        self.last_request_usage = load_latest_ai_usage(protocol).ok().flatten();
    }

    fn reload_filters(&mut self, cx: &mut Context<Self>) {
        let metadata_store = self.store.clone();
        cx.spawn(async move |view, cx| {
            let tags = metadata_store.get_all_tags().await;
            let persons = metadata_store.get_all_persons().await;

            let _ = view.update(cx, |panel, cx| {
                match (tags, persons) {
                    (Ok(tags), Ok(persons)) => {
                        panel.filters.replace_catalog(
                            tags.into_iter().map(|tag| tag.name).collect(),
                            persons.into_iter().map(|person| person.name).collect(),
                        );
                        panel.reload_preview(cx);
                    }
                    (tags_result, persons_result) => {
                        if let Err(err) = tags_result {
                            eprintln!("[AI Panel] Failed to load tags: {}", err);
                        }
                        if let Err(err) = persons_result {
                            eprintln!("[AI Panel] Failed to load persons: {}", err);
                        }
                    }
                }
                cx.notify();
            });
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
        let (tags, persons) = self.filters.build_query_values();
        Ok(AiContextQuery {
            start_at,
            end_at,
            tags,
            persons,
            keyword: self.keyword_input.read(cx).text().to_string(),
            record_scope: self.record_scope,
            user_request: self.current_user_request(cx),
            mode: self.mode,
        })
    }

    fn current_user_request(&self, cx: &App) -> String {
        let text = self.custom_request_input.read(cx).text().to_string();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            self.mode.default_request_template().to_string()
        } else {
            trimmed.to_string()
        }
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

    fn set_mode(&mut self, mode: AiSummaryMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        let request = mode.default_request_template();
        self.custom_request_input.update(cx, |input, cx| {
            input.replace_text_in_range(None, request, window, cx);
        });
        self.result = None;
        self.reload_preview(cx);
    }

    fn set_record_scope(&mut self, scope: AiRecordScope, cx: &mut Context<Self>) {
        if self.record_scope == scope {
            return;
        }
        self.record_scope = scope;
        self.result = None;
        self.reload_preview(cx);
    }

    fn toggle_tag(&mut self, tag: &str, cx: &mut Context<Self>) {
        self.filters.toggle_tag(tag);
        self.result = None;
        self.reload_preview(cx);
    }

    fn clear_tags(&mut self, cx: &mut Context<Self>) {
        if self.filters.selected_tags.is_empty() {
            return;
        }
        self.filters.selected_tags.clear();
        self.result = None;
        self.reload_preview(cx);
    }

    fn toggle_person(&mut self, person: &str, cx: &mut Context<Self>) {
        self.filters.toggle_person(person);
        self.result = None;
        self.reload_preview(cx);
    }

    fn clear_persons(&mut self, cx: &mut Context<Self>) {
        if self.filters.selected_persons.is_empty() {
            return;
        }
        self.filters.selected_persons.clear();
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
        let protocol = settings.protocol;
        let api_key = match load_secret(settings.protocol) {
            Ok(Some(secret)) => secret.value,
            Ok(None) => {
                self.generating = false;
                self.error = Some(format!(
                    "当前协议还没有配置 API Key，请先去设置里保存本地 Key，或提供环境变量 {}",
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
            let result = cx
                .background_executor()
                .spawn(async move { generate_summary(&settings, &api_key, &query, &bundle) })
                .await;
            let _ = view.update(cx, |panel, cx| {
                panel.generating = false;
                match result {
                    Ok(generation) => {
                        let usage_store_result = record_ai_usage(
                            protocol,
                            AiUsageEventKind::GenerateSummary,
                            generation.usage,
                        );
                        panel.result = Some(generation.text);
                        panel.notice = Some(match generation.usage {
                            Some(usage) => format!(
                                "AI 总结已生成，可以复制结果。本次上行 {}，下行 {}。",
                                usage.input_tokens, usage.output_tokens
                            ),
                            None => {
                                "AI 总结已生成，可以复制结果。当前响应未返回 usage。".to_string()
                            }
                        });
                        panel.error = None;
                        match usage_store_result {
                            Ok(_) => {
                                panel.refresh_usage_metrics();
                            }
                            Err(err) => {
                                panel.notice = None;
                                panel.error =
                                    Some(format!("AI 总结已生成，但写入 token 统计失败: {err}"));
                            }
                        }
                    }
                    Err(err) => {
                        let usage_store_result =
                            record_ai_usage(protocol, AiUsageEventKind::GenerateSummary, err.usage);
                        panel.result = None;
                        panel.notice = None;
                        panel.error = Some(match usage_store_result {
                            Ok(_) => {
                                panel.refresh_usage_metrics();
                                err.message
                            }
                            Err(record_err) => {
                                format!("{}；另外写入 token 统计失败: {}", err.message, record_err)
                            }
                        });
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

    fn toggle_samples(&mut self, cx: &mut Context<Self>) {
        self.samples_expanded = !self.samples_expanded;
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
            .on_click(cx.listener(move |this, _event, window, cx| {
                this.set_mode(mode, window, cx);
            }))
            .into_any_element()
    }

    fn scope_includes_tasks(&self) -> bool {
        self.record_scope != AiRecordScope::Records
    }

    fn scope_includes_records(&self) -> bool {
        self.record_scope != AiRecordScope::Tasks
    }

    fn toggle_task_scope(&mut self, cx: &mut Context<Self>) {
        let next = if self.record_scope == AiRecordScope::Records {
            AiRecordScope::All
        } else {
            AiRecordScope::Tasks
        };
        self.set_record_scope(next, cx);
    }

    fn toggle_record_scope(&mut self, cx: &mut Context<Self>) {
        let next = if self.record_scope == AiRecordScope::Tasks {
            AiRecordScope::All
        } else {
            AiRecordScope::Records
        };
        self.set_record_scope(next, cx);
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

    fn render_input_card(&self, cx: &mut Context<Self>) -> AnyElement {
        render_card(
            "输入",
            "",
            vec![
                self.render_section_title(
                    "需求",
                    Some("预设按钮会填入默认需求模板，你可以继续改写；最终仍会保留“仅基于上下文”的约束。"),
                ),
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
                    .into_any_element(),
                Input::new(&self.custom_request_input).into_any_element(),
                self.render_section_title(
                    "过滤",
                    Some("时间、标签、关键词和类型都会即时刷新命中结果。"),
                ),
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
                            .flex_col()
                            .gap(px(6.0))
                            .min_w(px(220.0))
                            .child(div().text_xs().text_color(rgb(0x595959)).child("关键词"))
                            .child(Input::new(&self.keyword_input).cleanable(true)),
                    )
                    .into_any_element(),
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(self.render_filter_chip(
                        "ai-scope-tasks",
                        "任务",
                        self.scope_includes_tasks(),
                        cx.listener(|this, _event, _window, cx| {
                            this.toggle_task_scope(cx);
                        }),
                    ))
                    .child(self.render_filter_chip(
                        "ai-scope-records",
                        "记录",
                        self.scope_includes_records(),
                        cx.listener(|this, _event, _window, cx| {
                            this.toggle_record_scope(cx);
                        }),
                    ))
                    .into_any_element(),
                self.render_filter_section(
                    "标签",
                    &self.filters.available_tags,
                    &self.filters.selected_tags,
                    "当前未建立标签。",
                    "清除标签",
                    cx,
                    MetadataFilterKind::Tag,
                ),
                self.render_filter_section(
                    "人物",
                    &self.filters.available_persons,
                    &self.filters.selected_persons,
                    "当前未建立人物。",
                    "清除人物",
                    cx,
                    MetadataFilterKind::Person,
                ),
                self.render_hits_section(cx),
                div()
                    .flex()
                    .justify_end()
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
        kind: MetadataFilterKind,
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
                    let label = match kind {
                        MetadataFilterKind::Tag => value.to_string(),
                        MetadataFilterKind::Person => format!("@{}", value),
                    };
                    let id = match kind {
                        MetadataFilterKind::Tag => format!("ai-tag-{idx}"),
                        MetadataFilterKind::Person => format!("ai-person-{idx}"),
                    };
                    self.render_filter_chip(
                        id,
                        &label,
                        selected.contains(value),
                        cx.listener({
                            let value = value.clone();
                            move |this, _event, _window, cx| match kind {
                                MetadataFilterKind::Tag => this.toggle_tag(&value, cx),
                                MetadataFilterKind::Person => this.toggle_person(&value, cx),
                            }
                        }),
                    )
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
                                .on_click(cx.listener(
                                    move |this, _event, _window, cx| match kind {
                                        MetadataFilterKind::Tag => this.clear_tags(cx),
                                        MetadataFilterKind::Person => this.clear_persons(cx),
                                    },
                                )),
                        )
                    }),
            )
            .child(chips)
            .into_any_element()
    }

    fn render_status_bar(&self) -> AnyElement {
        let config_ready = self.config.settings.has_connection_config() && self.config.has_api_key;
        let (status_text, status_color, status_bg, status_border) = if config_ready {
            ("已连接", rgb(0x389e0d), rgb(0xf6ffed), rgb(0xb7eb8f))
        } else {
            ("未完成", rgb(0xcf1322), rgb(0xfff2f0), rgb(0xffccc7))
        };
        let model_label = if self.config.settings.model.trim().is_empty() {
            "未设置模型".to_string()
        } else {
            format!(
                "{} · {}",
                self.config.settings.protocol.label(),
                self.config.settings.model.trim()
            )
        };
        let source_label = match self.config.source.as_ref() {
            Some(SecretSource::LocalFile) => "本地 Key",
            Some(SecretSource::Environment(_)) => "环境变量",
            None => "未配置 Key",
        };
        let today_usage = self.today_usage.as_ref().map(|entry| entry.usage());
        let last_usage = self
            .last_request_usage
            .as_ref()
            .and_then(|snapshot| snapshot.usage);

        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(999.0))
                    .bg(status_bg)
                    .border_1()
                    .border_color(status_border)
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(status_color)
                            .child(status_text),
                    )
                    .child(render_info_icon(
                        "仅表示已检测到当前协议的配置和可用 Key，不代表远端连通性已经验证通过。",
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x595959))
                            .child(model_label),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x8c8c8c))
                            .child(source_label),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x8c8c8c))
                    .child(format!(
                        "今日 ↑{} ↓{}",
                        format_usage_value(today_usage.map(|usage| usage.input_tokens)),
                        format_usage_value(today_usage.map(|usage| usage.output_tokens))
                    )),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x8c8c8c))
                    .child(format!(
                        "本次 ↑{} ↓{}",
                        format_usage_value(last_usage.map(|usage| usage.input_tokens)),
                        format_usage_value(last_usage.map(|usage| usage.output_tokens))
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(render_info_icon(
                        "仅统计 Robinne 发起的请求。服务端未返回 usage 时不会估算，也不会计入今日累计。",
                    )),
            )
            .into_any_element()
    }

    fn render_section_title(
        &self,
        title: &'static str,
        tooltip: Option<&'static str>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0x595959))
                    .child(title),
            )
            .when_some(tooltip, |this, tooltip| {
                this.child(render_info_icon(tooltip))
            })
            .into_any_element()
    }

    fn render_hits_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let content = if self.preview_loading {
            div()
                .text_sm()
                .text_color(rgb(0x8c8c8c))
                .child("正在更新命中…")
                .into_any_element()
        } else if let Some(preview) = self.preview.as_ref() {
            let sample_records = if self.samples_expanded {
                div().into_any_element()
            } else {
                div().into_any_element()
            };

            div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x595959))
                        .line_height(relative(1.6))
                        .child(format!(
                            "命中 {} 条，任务 {} 条，记录 {} 条，时间范围 {}。",
                            preview.records.len(),
                            preview.task_count,
                            preview.note_like_count,
                            preview.date_span_label
                        )),
                )
                .when(!preview.records.is_empty(), |this| {
                    this.child(
                        Button::new("ai-toggle-samples")
                            .child(if self.samples_expanded {
                                "关闭样本"
                            } else {
                                "查看样本"
                            })
                            .small()
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                cx.stop_propagation();
                                this.toggle_samples(cx);
                            })),
                    )
                })
                .child(sample_records)
                .into_any_element()
        } else {
            div()
                .text_sm()
                .text_color(rgb(0x8c8c8c))
                .child("当前没有命中内容。")
                .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(self.render_section_title(
                "命中内容",
                Some("这里只展示本地筛选结果摘要。展开后可快速检查命中样本。"),
            ))
            .child(content)
            .into_any_element()
    }

    fn render_output_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let header =
            div()
                .flex()
                .gap(px(12.0))
                .when(self.result.is_some(), |this| {
                    this.items_center().justify_between()
                })
                .when(self.result.is_none(), |this| this.flex_col())
                .child(div().into_any_element())
                .when(self.result.is_some(), |this| {
                    this.child(Button::new("ai-copy-result").child("复制结果").on_click(
                        cx.listener(|this, _event, _window, cx| {
                            this.copy_result(cx);
                        }),
                    ))
                });

        let body = if self.generating {
            div()
                .min_h(px(220.0))
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
                        .child("结果会直接显示在这里。"),
                )
                .into_any_element()
        } else if let Some(result) = self.result.as_ref() {
            div()
                .min_h(px(220.0))
                .max_h(px(760.0))
                .overflow_y_scrollbar()
                .child(
                    div()
                        .text_sm()
                        .line_height(relative(1.7))
                        .text_color(rgb(0x262626))
                        .child(result.clone()),
                )
                .into_any_element()
        } else {
            div()
                .min_h(px(220.0))
                .flex()
                .flex_col()
                .justify_center()
                .gap(px(14.0))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0x8c8c8c))
                        .child("生成结果会显示在这里"),
                )
                .into_any_element()
        };

        render_card("输出", "", vec![header.into_any_element(), body])
    }

    fn render_samples_sidebar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let preview = self.preview.as_ref()?;
        if !self.samples_expanded || preview.records.is_empty() {
            return None;
        }

        Some(
            div()
                .id("ai-samples-sidebar")
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .flex()
                .flex_row()
                .justify_end()
                .child(
                    div()
                        .id("ai-samples-sidebar-dismiss-area")
                        .flex_1()
                        .h_full()
                        .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            this.toggle_samples(cx);
                        })),
                )
                .child(
                    div()
                        .id("ai-samples-sidebar-pane")
                        .w(px(360.0))
                        .h_full()
                        .min_h(px(0.0))
                        .flex()
                        .flex_col()
                        .occlude()
                        .overflow_hidden()
                        .border_l_1()
                        .border_color(rgb(0xe8e8e8))
                        .bg(rgb(0xffffff))
                        .on_click(cx.listener(|_this, _event: &ClickEvent, _window, cx| {
                            cx.stop_propagation();
                        }))
                        .child(
                            div()
                                .p(px(12.0))
                                .border_b_1()
                                .border_color(rgb(0xe8e8e8))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(rgb(0x262626))
                                                .child("命中样本"),
                                        )
                                        .child(
                                            Button::new("ai-close-samples").child("✕").on_click(
                                                cx.listener(|this, _event, _window, cx| {
                                                    cx.stop_propagation();
                                                    this.toggle_samples(cx);
                                                }),
                                            ),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .p(px(12.0))
                                .text_xs()
                                .text_color(rgb(0x8c8c8c))
                                .child(format!("共 {} 条命中内容", preview.records.len())),
                        )
                        .child(
                            div().flex_1().min_h_0().overflow_hidden().child(
                                v_flex()
                                    .p(px(12.0))
                                    .gap(px(10.0))
                                    .overflow_y_scrollbar()
                                    .children(preview.records.iter().map(render_record_sample)),
                            ),
                        ),
                )
                .into_any_element(),
        )
    }
}

impl Render for AiPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .child(
                div()
                    .id("ai-panel-main")
                    .size_full()
                    .p(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                        if this.samples_expanded {
                            this.toggle_samples(cx);
                        }
                    }))
                    .overflow_y_scrollbar()
                    .child(
                        div()
                            .flex()
                            .min_w(px(0.0))
                            .flex_col()
                            .items_start()
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
                                    .child(self.render_status_bar()),
                            ),
                    )
                    .when_some(self.render_message(), |this, message| this.child(message))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(16.0))
                            .child(self.render_input_card(cx))
                            .child(self.render_output_card(cx)),
                    ),
            )
            .when_some(self.render_samples_sidebar(cx), |this, sidebar| {
                this.child(sidebar)
            })
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
                .when(!description.is_empty(), |this| {
                    this.child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x8c8c8c))
                            .line_height(relative(1.5))
                            .child(description),
                    )
                }),
        )
        .children(children)
        .into_any_element()
}

fn render_info_icon(message: &'static str) -> AnyElement {
    Button::new(format!("ai-info-{message}"))
        .ghost()
        .small()
        .icon(IconName::Info)
        .tooltip(message)
        .into_any_element()
}

fn format_usage_value(value: Option<u64>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => "暂无".to_string(),
    }
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

#[cfg(test)]
mod tests {
    use super::MetadataFilters;
    use std::collections::BTreeSet;

    #[test]
    fn metadata_filters_build_query_values_include_selected_persons() {
        let filters = MetadataFilters {
            selected_tags: BTreeSet::from(["开发".to_string()]),
            selected_persons: BTreeSet::from(["张三".to_string(), "李四".to_string()]),
            ..Default::default()
        };

        let (tags, persons) = filters.build_query_values();

        assert_eq!(tags, vec!["开发".to_string()]);
        assert_eq!(persons, vec!["张三".to_string(), "李四".to_string()]);
    }

    #[test]
    fn metadata_filters_replace_catalog_refreshes_person_options_and_drops_missing_selection() {
        let mut filters = MetadataFilters {
            selected_tags: BTreeSet::from(["保留标签".to_string(), "移除标签".to_string()]),
            selected_persons: BTreeSet::from(["保留人物".to_string(), "移除人物".to_string()]),
            ..Default::default()
        };

        filters.replace_catalog(
            vec!["保留标签".to_string(), "新增标签".to_string()],
            vec!["保留人物".to_string(), "新增人物".to_string()],
        );

        assert_eq!(
            filters.available_tags,
            vec!["保留标签".to_string(), "新增标签".to_string()]
        );
        assert_eq!(
            filters.available_persons,
            vec!["保留人物".to_string(), "新增人物".to_string()]
        );
        assert_eq!(
            filters.selected_tags,
            BTreeSet::from(["保留标签".to_string()])
        );
        assert_eq!(
            filters.selected_persons,
            BTreeSet::from(["保留人物".to_string()])
        );
    }
}
