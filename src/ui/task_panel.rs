use crate::models::{Priority, Record};
use crate::store::Store;
use chrono::{Datelike, Duration, Local, TimeZone, Timelike};
use gpui::*;
use gpui::prelude::FluentBuilder as _;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::button::Button;
use gpui_component::date_picker::{DatePicker, DatePickerState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::{h_flex, v_flex, IconName};
use uuid::Uuid;

actions!(task_panel, [
    EditTaskAction, 
    DeleteTaskAction, 
    SetReminderAction,
    SetReminderTodayAction,
    SetReminderTomorrowAction,
    SetReminderNextWeekAction
]);

pub struct TaskPanel {
    store: Store,
    tasks: Vec<Record>,
    input_state: Entity<InputState>,
    _subscription: Subscription,
    // 编辑状态
    editing_task_id: Option<uuid::Uuid>,
    edit_input_state: Option<Entity<InputState>>,
    _edit_subscription: Option<Subscription>,
    context_menu_task_id: Option<uuid::Uuid>,
    context_menu_position: Option<Point<Pixels>>,
    // 提醒设置 Popover 状态
    reminder_task_id: Option<uuid::Uuid>,
    reminder_date_picker: Option<Entity<DatePickerState>>,
    reminder_time_input: Option<Entity<InputState>>,
    reminder_error_message: Option<String>,
    _reminder_subscriptions: Vec<Subscription>,
    _window_activation_subscription: Subscription,
    // 显示状态
    show_completed: bool,
}

impl TaskPanel {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("!! 高优先级 | ! 普通优先级 | 直接输入")
        });

        let _subscription = cx.subscribe_in(
            &input_state,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.create_task(window, cx);
                }
            },
        );

        let mut panel = Self {
            store,
            tasks: Vec::new(),
            input_state,
            _subscription,
            editing_task_id: None,
            edit_input_state: None,
            _edit_subscription: None,
            context_menu_task_id: None,
            context_menu_position: None,
            reminder_task_id: None,
            reminder_date_picker: None,
            reminder_time_input: None,
            reminder_error_message: None,
            _reminder_subscriptions: Vec::new(),
            _window_activation_subscription: cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.load_tasks(cx);
                }
            }),
            show_completed: false,
        };
        panel.load_tasks(cx);
        panel
    }

    fn start_edit(&mut self, task: Record, window: &mut Window, cx: &mut Context<Self>) {
        self.editing_task_id = Some(task.id);
        let edit_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("编辑任务...")
        });
        // 设置编辑内容 - 使用 value 方式
        let content = task.content.clone();
        edit_input.update(cx, |state, cx| {
            state.set_value(&content, window, cx);
            state.focus(window, cx);
        });
        eprintln!("[TaskPanel] edit_input created, editing_task_id set to: {:?}", self.editing_task_id);

        let _edit_subscription = cx.subscribe_in(
            &edit_input,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => {
                        this.save_edit(window, cx);
                    }
                    InputEvent::Blur => {
                        // 失去焦点自动保存 (滴答清单风格)
                        this.save_edit(window, cx);
                    }
                    _ => {}
                }
            },
        );

        self.edit_input_state = Some(edit_input);
        self._edit_subscription = Some(_edit_subscription);
        cx.notify();
    }

    fn save_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(task_id) = self.editing_task_id {
            if let Some(edit_input) = &self.edit_input_state {
                let new_content = edit_input.read(cx).text().to_string();
                let (content, priority) = Self::parse_input_static(&new_content);

                if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    task.content = content;
                    task.priority = Some(priority);
                    let updated_task = task.clone();
                    let store = self.store.clone();
                    cx.spawn(async move |_view, _cx| {
                        if let Err(e) = store.update_record(updated_task).await {
                            eprintln!("[TaskPanel] Failed to update task: {}", e);
                        }
                    }).detach();
                }
            }
        }
        self.cancel_edit(window, cx);
    }

    fn cancel_edit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.editing_task_id = None;
        self.edit_input_state = None;
        self._edit_subscription = None;
        cx.notify();
    }

    fn start_reminder(&mut self, task: &Record, window: &mut Window, cx: &mut Context<Self>) {
        let task_id = task.id;
        self.reminder_task_id = Some(task_id);

        // 如果任务已有提醒时间，回显已保存的日期和时间；否则用当前时间
        let (init_date, init_time_str) = if let Some(scheduled) = task.scheduled_for {
            let local = scheduled.with_timezone(&chrono::Local);
            (local.naive_local().date(), local.format("%H:%M").to_string())
        } else {
            let now = chrono::Local::now();
            (now.naive_local().date(), now.format("%H:%M").to_string())
        };

        // 创建 DatePicker 状态
        let date_picker = cx.new(|cx| {
            let mut picker = DatePickerState::new(window, cx)
                .date_format("%Y-%m-%d");
            picker.set_date(init_date, window, cx);
            picker
        });

        // 创建时间输入框
        let time_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("HH:MM 如 14:30")
        });
        time_input.update(cx, |state, cx| {
            state.set_value(&init_time_str, window, cx);
        });

        self._reminder_subscriptions.clear();
        self.reminder_error_message = None;
        self.reminder_date_picker = Some(date_picker);
        self.reminder_time_input = Some(time_input);
        cx.notify();
    }

    fn save_reminder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let (Some(task_id), Some(date_picker), Some(time_input)) = (
            self.reminder_task_id,
            self.reminder_date_picker.as_ref(),
            self.reminder_time_input.as_ref(),
        ) {
            let date = date_picker.read(cx).date();
            let time_str = time_input.read(cx).text().to_string();

            // 解析日期
            if let Some(naive_date) = date.start() {
                // 严格验证和解析时间 HH:MM
                let parts: Vec<&str> = time_str.split(':').collect();
                let is_valid = parts.len() == 2 && parts[0].len() <= 2 && parts[1].len() == 2;
                
                let time_parsed = if is_valid {
                    match (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                        (Ok(h), Ok(m)) if h <= 23 && m <= 59 => Some((h, m)),
                        _ => None,
                    }
                } else {
                    None
                };

                if let Some((hour, minute)) = time_parsed {
                    self.reminder_error_message = None;
                    if let Some(naive_dt) = naive_date.and_hms_opt(hour, minute, 0) {
                        if let Some(local_dt) = chrono::Local.from_local_datetime(&naive_dt).single() {
                            let utc_dt = local_dt.with_timezone(&chrono::Utc);

                            if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                                task.scheduled_for = Some(utc_dt);
                                task.notified_at = None;
                                let updated_task = task.clone();
                                let store = self.store.clone();
                                cx.spawn(async move |_view, _cx| {
                                    if let Err(e) = store.update_record(updated_task).await {
                                        eprintln!("[TaskPanel] Failed to update task reminder: {}", e);
                                    }
                                }).detach();
                            }
                        }
                    }
                } else {
                    self.reminder_error_message = Some("时间格式无效，请输入 HH:MM（例 14:30）".to_string());
                    cx.notify();
                    return; // 阻止保存，且不关闭弹窗
                }
            } else {
                self.reminder_error_message = Some("请选择日期".to_string());
                cx.notify();
                return;
            }
        }
        self.cancel_reminder(window, cx);
    }

    fn update_task_reminder(&mut self, task_id: Uuid, scheduled_for: chrono::DateTime<chrono::Utc>, cx: &mut Context<Self>) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.scheduled_for = Some(scheduled_for);
            let updated_task = task.clone();
            let store = self.store.clone();
            cx.spawn(async move |_view, _cx| {
                if let Err(e) = store.update_record(updated_task).await {
                    eprintln!("[TaskPanel] Failed to update reminder: {}", e);
                }
            }).detach();
            cx.notify();
        }
    }

    fn cancel_reminder(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.reminder_task_id = None;
        self.reminder_date_picker = None;
        self.reminder_time_input = None;
        self.reminder_error_message = None;
        self._reminder_subscriptions.clear();
        cx.notify();
    }

    fn load_tasks(&mut self, cx: &mut Context<Self>) {
        eprintln!("[TaskPanel] load_tasks called");
        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            // 重试机制：数据库可能还未初始化
            let mut retries = 0;
            let tasks = loop {
                eprintln!("[TaskPanel] Fetching tasks from store... (attempt {})", retries + 1);
                match store.get_tasks(false).await {
                    Ok(tasks) => break tasks,
                    Err(e) => {
                        eprintln!("[TaskPanel] Failed to load tasks: {}, retrying...", e);
                        retries += 1;
                        if retries >= 3 {
                            eprintln!("[TaskPanel] Max retries reached, giving up");
                            break Vec::new();
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            };

            eprintln!("[TaskPanel] Loaded {} tasks", tasks.len());
            let update_result = view.update(cx, |panel, cx| {
                panel.tasks = tasks;
                cx.notify();  // 触发界面刷新
                eprintln!("[TaskPanel] Tasks updated and notified, panel now has {} tasks", panel.tasks.len());
            });
            if let Err(e) = update_result {
                eprintln!("[TaskPanel] Failed to update view: {:?}", e);
            }
        }).detach();
    }

    fn create_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input_state.read(cx).text().to_string();
        eprintln!("[TaskPanel] create_task called with text: '{}'", text);
        if text.trim().is_empty() {
            eprintln!("[TaskPanel] Text is empty, returning");
            return;
        }

        let (content, priority) = self.parse_input(&text);
        eprintln!("[TaskPanel] Parsed content: '{}', priority: {:?}", content, priority);
        let task = Record::new_task(content, priority);
        eprintln!("[TaskPanel] Created task with id: {}", task.id);

        self.input_state.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });

        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            eprintln!("[TaskPanel] Spawning create_record...");
            match store.create_record(task).await {
                Ok(_) => {
                    eprintln!("[TaskPanel] create_record succeeded, scheduling load_tasks");
                    // 延迟一点再加载，确保数据库写入完成
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    let update_result = view.update(cx, |panel, cx| {
                        eprintln!("[TaskPanel] About to call load_tasks from create_task callback");
                        panel.load_tasks(cx);
                        eprintln!("[TaskPanel] load_tasks called from callback");
                    });
                    if let Err(e) = update_result {
                        eprintln!("[TaskPanel] Failed to update view: {:?}", e);
                    } else {
                        eprintln!("[TaskPanel] View update succeeded");
                    }
                }
                Err(e) => eprintln!("[TaskPanel] Failed to create task: {}", e),
            }
        }).detach();
    }

    fn parse_input(&self, input: &str) -> (String, Priority) {
        Self::parse_input_static(input)
    }

    // 提取为静态方法便于测试
    fn parse_input_static(input: &str) -> (String, Priority) {
        let trimmed = input.trim();
        if let Some(rest) = trimmed.strip_prefix("!!").or_else(|| trimmed.strip_prefix("！！")) {
            (rest.trim_start().to_string(), Priority::High)
        } else if let Some(rest) = trimmed.strip_prefix("!").or_else(|| trimmed.strip_prefix("！")) {
            (rest.trim_start().to_string(), Priority::Medium)
        } else {
            (trimmed.to_string(), Priority::Low)
        }
    }

    fn toggle_task_complete(&mut self, task_id: Uuid, cx: &mut Context<Self>) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            // 切换完成状态
            if task.completed_at.is_some() {
                task.completed_at = None;
            } else {
                task.completed_at = Some(chrono::Utc::now());
            }
            let updated_task = task.clone();
            let store = self.store.clone();

            cx.spawn(async move |view, cx| {
                match store.update_record(updated_task).await {
                    Ok(_) => {
                        view.update(cx, |panel, cx| {
                            panel.load_tasks(cx);
                        }).ok();
                    }
                    Err(e) => eprintln!("Failed to toggle task: {}", e),
                }
            }).detach();
        }
    }

    fn delete_task(&mut self, task_id: Uuid, cx: &mut Context<Self>) {
        let store = self.store.clone();
        
        cx.spawn(async move |view, cx| {
            match store.delete_record(task_id).await {
                Ok(_) => {
                    view.update(cx, |panel, cx| {
                        panel.load_tasks(cx);
                    }).ok();
                }
                Err(e) => eprintln!("Failed to delete task: {}", e),
            }
        }).detach();
    }

    fn render_custom_context_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (_task_id, position) = match (self.context_menu_task_id, self.context_menu_position) {
            (Some(id), Some(pos)) => (id, pos),
            _ => return None,
        };

        Some(
            deferred(
                anchored()
                    .position(position)
                    .child(
                        v_flex()
                            .id("custom-context-menu")
                            .bg(rgb(0xffffff))
                            .border_1()
                            .border_color(rgb(0xe0e0e0))
                            .rounded(px(8.0))
                            .shadow_md()
                            .min_w(px(200.0))
                            .on_mouse_down_out(cx.listener(|this, _event, _window, cx| {
                                this.context_menu_task_id = None;
                                this.context_menu_position = None;
                                cx.notify();
                            }))
                            // 顶部快捷行
                            .child(
                                v_flex()
                                    .p(px(8.0))
                                    .child(div().text_xs().text_color(rgb(0xaaaaaa)).mb_1().child("日期"))
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .child(self.render_menu_shortcut(IconName::Sun, SetReminderTodayAction, cx))
                                            .child(self.render_menu_shortcut(IconName::Bell, SetReminderTomorrowAction, cx))
                                            .child(self.render_menu_shortcut(IconName::Calendar, SetReminderNextWeekAction, cx))
                                            .child(self.render_menu_shortcut(IconName::Plus, SetReminderAction, cx))
                                    )
                            )
                            .child(div().h_px().bg(rgb(0xeeeeee)))
                            // 功能列表
                            .child(self.render_menu_item("设置提醒", IconName::Bell, SetReminderAction, cx))
                            .child(self.render_menu_item("编辑", IconName::Settings, EditTaskAction, cx))
                            .child(div().h_px().bg(rgb(0xeeeeee)))
                            .child(self.render_menu_item("删除", IconName::Delete, DeleteTaskAction, cx))
                    )
            ).into_any()
        )
    }

    fn render_menu_shortcut<A: Action + 'static>(&self, icon: IconName, action: A, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(("shortcut", icon.clone() as usize))
            .p(px(6.0))
            .rounded(px(4.0))
            .hover(|s| s.bg(rgb(0xf5f5f5)))
            .cursor_pointer()
            .on_click(cx.listener(move |_, _event, window, cx| {
                window.dispatch_action(action.boxed_clone(), cx);
            }))
            .child(
                gpui_component::Icon::new(icon)
                    .size(px(18.0))
                    .text_color(rgb(0x666666))
            )
    }

    fn render_menu_item<A: Action + 'static>(&self, label: &'static str, icon: IconName, action: A, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id(label)
            .w_full()
            .px(px(12.0))
            .py(px(8.0))
            .gap(px(10.0))
            .hover(|s| s.bg(rgb(0xf5f5f5)))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _event, window, cx| {
                this.context_menu_task_id = None;
                this.context_menu_position = None;
                window.dispatch_action(action.boxed_clone(), cx);
                cx.notify();
            }))
            .child(
                gpui_component::Icon::new(icon)
                    .size(px(16.0))
                    .text_color(rgb(0x666666))
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x333333))
                    .child(label)
            )
    }
}

impl Render for TaskPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 分离已完成和未完成任务
        let (pending_tasks, completed_tasks): (Vec<_>, Vec<_>) = self.tasks.iter()
            .cloned()
            .partition(|task| task.completed_at.is_none());

        let pending_count = pending_tasks.len();
        let completed_count = completed_tasks.len();

        eprintln!("[TaskPanel] Rendering {} pending, {} completed", pending_count, completed_count);

        // 渲染单个任务项的辅助函数
        let mut render_task = |task: Record, idx: usize, cx: &mut Context<Self>| {
            let task_id = task.id;
            let is_completed = task.completed_at.is_some();
            let priority_emoji = match task.priority {
                Some(Priority::High) => "🔴",
                Some(Priority::Medium) => "🟡",
                Some(Priority::Low) => "🟢",
                None => "⚪",
            };


            // 格式化简短日期
            let fmt_short = |dt: chrono::DateTime<chrono::Utc>| -> String {
                let local = dt.with_timezone(&chrono::Local);
                let now = chrono::Local::now();
                if local.date_naive() == now.date_naive() {
                    format!("今天 {}", local.format("%H:%M"))
                } else if local.year() == now.year() {
                    format!("{}月{}日 {}", local.month(), local.day(), local.format("%H:%M"))
                } else {
                    local.format("%Y-%m-%d %H:%M").to_string()
                }
            };


            div()
                .id(("task", idx))
                .flex()
                .gap(px(8.0))
                .items_center()
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(6.0))
                .bg(if is_completed { rgb(0xf0f0f0) } else { rgb(0xfafafa) })
                .border_1()
                .border_color(rgb(0xe8e8e8))
                .hover(|style| style.bg(if is_completed { rgb(0xe8e8e8) } else { rgb(0xf0f5ff) }))
                .text_color(rgb(0x000000))
                // 右键点击时记住目标任务 ID 和位置
                .on_mouse_down(MouseButton::Right, cx.listener({
                    let task_id = task_id;
                    move |this, event: &MouseDownEvent, _window, cx| {
                        this.context_menu_task_id = Some(task_id);
                        this.context_menu_position = Some(event.position);
                        cx.notify();
                    }
                }))
                .child(
                    div()
                        .cursor_pointer()
                        .child(if is_completed { "☑" } else { "☐" })
                        .id(("checkbox", idx))
                        .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                            this.toggle_task_complete(task_id, cx);
                        }))
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child({
                            let is_editing = self.editing_task_id == Some(task.id);
                            let content = task.content.clone();
                            
                            let input_state = if is_editing {
                                self.edit_input_state.as_ref().unwrap().clone()
                            } else {
                                cx.new(|cx| {
                                    let mut state = InputState::new(window, cx);
                                    state.set_value(&content, window, cx);
                                    state
                                })
                            };
                            
                            div()
                                .id(("task-title", idx))
                                .flex_1()
                                .px(px(4.0))
                                .py(px(2.0))
                                .text_color(if is_completed { rgb(0x999999) } else { rgb(0x1a1a1a) })
                                .when(is_completed && !is_editing, |el| el.line_through())
                                .cursor_pointer()
                                .on_click(cx.listener({
                                    let task = task.clone();
                                    move |this, _event: &ClickEvent, window, cx| {
                                        if this.editing_task_id != Some(task.id) {
                                            this.start_edit(task.clone(), window, cx);
                                        }
                                    }
                                }))
                                .child(
                                    Input::new(&input_state)
                                        .appearance(false)
                                        .focus_bordered(false)
                                        .p(px(0.0))
                                        .when(!is_editing, |input| input.disabled(true))
                                )
                                .into_any_element()
                        })
                        .child(
                            div()
                                .flex()
                                .gap(px(8.0))
                                .text_color(rgb(0xbbbbbb))
                                .text_xs()
                                .child(fmt_short(task.created_at))
                                .children(task.scheduled_for.map(|t| {
                                    div()
                                        .text_color(rgb(0x1890ff))
                                        .child(format!("📅 {}", fmt_short(t)))
                                }))
                                .children(task.due_date.map(|t| {
                                    div()
                                        .text_color(rgb(0xff4d4f))
                                        .child(format!("⏰ {}", fmt_short(t)))
                                }))
                                .children(task.completed_at.map(|t| {
                                    div()
                                        .text_color(rgb(0x52c41a))
                                        .child(format!("✓ {}", fmt_short(t)))
                                }))
                        )
                )
                // 优先级圆点（右侧）
                .child(
                    div()
                        .text_sm()
                        .child(priority_emoji)
                )
                .into_any_element()
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .on_action(cx.listener(|this, _action: &SetReminderAction, window, cx| {
                if let Some(task_id) = this.context_menu_task_id {
                    if let Some(task) = this.tasks.iter().find(|t| t.id == task_id).cloned() {
                        this.start_reminder(&task, window, cx);
                    }
                }
            }))
            .on_action(cx.listener(|this, _action: &SetReminderTodayAction, _window, cx| {
                if let Some(task_id) = this.context_menu_task_id {
                    let now = Local::now();
                    // 如果现在多于 17点，设为 1小时后，否则设为今天 18点
                    let target = if now.hour() >= 17 {
                        now + Duration::hours(1)
                    } else {
                        now.date_naive().and_hms_opt(18, 0, 0).unwrap().and_local_timezone(Local).unwrap()
                    };
                    this.update_task_reminder(task_id, target.with_timezone(&chrono::Utc), cx);
                }
            }))
            .on_action(cx.listener(|this, _action: &SetReminderTomorrowAction, _window, cx| {
                if let Some(task_id) = this.context_menu_task_id {
                    let tomorrow = (Local::now() + Duration::days(1)).date_naive();
                    let target = tomorrow.and_hms_opt(9, 0, 0).unwrap().and_local_timezone(Local).unwrap();
                    this.update_task_reminder(task_id, target.with_timezone(&chrono::Utc), cx);
                }
            }))
            .on_action(cx.listener(|this, _action: &SetReminderNextWeekAction, _window, cx| {
                if let Some(task_id) = this.context_menu_task_id {
                    let next_week = (Local::now() + Duration::days(7)).date_naive();
                    let target = next_week.and_hms_opt(9, 0, 0).unwrap().and_local_timezone(Local).unwrap();
                    this.update_task_reminder(task_id, target.with_timezone(&chrono::Utc), cx);
                }
            }))
            .on_action(cx.listener(|this, _action: &EditTaskAction, window, cx| {
                if let Some(task_id) = this.context_menu_task_id {
                    if let Some(task) = this.tasks.iter().find(|t| t.id == task_id).cloned() {
                        this.start_edit(task, window, cx);
                    }
                }
            }))
            .on_action(cx.listener(|this, _action: &DeleteTaskAction, _window, cx| {
                if let Some(task_id) = this.context_menu_task_id {
                    this.delete_task(task_id, cx);
                }
            }))
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("任务 ({} 待办 / {} 已完成)", pending_count, completed_count))
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                if event.keystroke.key == "enter" {
                                    this.create_task(window, cx);
                                }
                            }))
                            .child(Input::new(&self.input_state))
                    )
                    .child(
                        Button::new("add-btn")
                            .child("添加")
                            .on_click(cx.listener(|this, _event: &ClickEvent, window, cx| {
                                this.create_task(window, cx);
                            }))
                    )
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x888888))
                    .child("输入格式: !! 高优先级 | ! 普通优先级 | 直接输入为低优先级")
            )
            // 提醒设置卡片（内联在任务列表上方）
            .when(self.reminder_task_id.is_some(), |el: Div| {
                if let (Some(ref dp), Some(ref ti)) = (&self.reminder_date_picker, &self.reminder_time_input) {
                    let dp_clone = dp.clone();
                    let ti_clone = ti.clone();
                    let task_name = self.reminder_task_id
                        .and_then(|id| self.tasks.iter().find(|t| t.id == id))
                        .map(|t| t.content.clone())
                        .unwrap_or_default();
                    el.child(
                        div()
                            .p(px(16.0))
                            .mx(px(4.0))
                            .my(px(8.0))
                            .bg(rgb(0xf0f5ff))
                            .border_1()
                            .border_color(rgb(0xadc6ff))
                            .rounded(px(8.0))
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("⏰ 设置提醒时间")
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(0x666666))
                                            .child(task_name)
                                    )
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(12.0))
                                    .items_end()
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.0))
                                            .child(div().text_xs().text_color(rgb(0x888888)).child("日期"))
                                            .child(
                                                DatePicker::new(&dp_clone)
                                                    .cleanable(true)
                                                    .number_of_months(1)
                                            )
                                    )
                                    .child(
                                        div()
                                            .w(px(100.0))
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.0))
                                            .child(div().text_xs().text_color(rgb(0x888888)).child("时间 (HH:MM)"))
                                            .child(
                                                Input::new(&ti_clone)
                                            )
                                    )
                            )
                            // 错误提示区
                            .children(self.reminder_error_message.as_ref().map(|err_msg| {
                                div()
                                    .text_sm()
                                    .text_color(rgb(0xff4d4f))
                                    .child(err_msg.clone())
                            }))
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap(px(8.0))
                                    .child(
                                        Button::new("cancel-reminder")
                                            .child("取消")
                                            .on_click(cx.listener(|this, _event, window, cx| {
                                                this.cancel_reminder(window, cx);
                                            }))
                                    )
                                    .child(
                                        Button::new("save-reminder")
                                            .child("设定")
                                            .on_click(cx.listener(|this, _event, window, cx| {
                                                this.save_reminder(window, cx);
                                            }))
                                    )
                            )
                    )
                } else {
                    el
                }
            })
            // 待办任务区域
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .id("task-list")
                            .size_full()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .pr(px(16.0))
                            .overflow_y_scrollbar()
                            .children({
                                let mut elements: Vec<AnyElement> = Vec::new();

                        // 待办任务标题
                        if pending_count > 0 {
                            elements.push(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0x666666))
                                    .child(format!("待办任务 ({})", pending_count))
                                    .into_any_element()
                            );
                            // 待办任务列表
                            for (idx, task) in pending_tasks.iter().cloned().enumerate() {
                                elements.push(render_task(task, idx, cx));
                            }
                        }

                        // 已完成任务标题和列表
                        if completed_count > 0 {
                            elements.push(
                                div()
                                    .id("completed-header")
                                    .mt(px(16.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                        this.show_completed = !this.show_completed;
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(rgb(0x999999))
                                                    .child(format!("{} 已完成 ({})", if self.show_completed { "▼" } else { "▶" }, completed_count))
                                            )
                                    )
                                    .into_any_element()
                            );
                            
                            if self.show_completed {
                                let pending_len = pending_tasks.len();
                                for (idx, task) in completed_tasks.iter().cloned().enumerate() {
                                    elements.push(render_task(task, pending_len + idx, cx));
                                }
                            }
                        }

                        elements
                    })
            )
            // 自定义右键菜单
            .children(self.render_custom_context_menu(cx))
        )
    }
}
