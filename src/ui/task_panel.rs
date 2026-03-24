use crate::models::{Priority, Record, TaskStatus};
use crate::store::Store;
use crate::ui::task_detail_sidebar::TaskDetailSidebar;
use chrono::{Datelike, Duration, Local, TimeZone, Timelike, Utc};
use gpui::*;
use gpui::prelude::FluentBuilder as _;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::date_picker::{DatePicker, DatePickerState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::{h_flex, v_flex, IconName};
use std::collections::HashMap;
use uuid::Uuid;

actions!(task_panel, [
    EditTaskAction, 
    DeleteTaskAction, 
    SetReminderAction,
    SetReminderTodayAction,
    SetReminderTomorrowAction,
    SetReminderNextWeekAction
]);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TaskView {
    List,
    Matrix,
}

impl TaskView {
    fn label(&self) -> &'static str {
        match self {
            TaskView::List => "列表视图",
            TaskView::Matrix => "矩阵视图",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Quadrant {
    UrgentImportant,
    NotUrgentImportant,
    UrgentNotImportant,
    NotUrgentNotImportant,
}

impl Quadrant {
    fn label(&self) -> &'static str {
        match self {
            Quadrant::UrgentImportant => "重要且紧急",
            Quadrant::NotUrgentImportant => "重要不紧急",
            Quadrant::UrgentNotImportant => "不重要紧急",
            Quadrant::NotUrgentNotImportant => "不重要不紧急",
        }
    }

    fn color(&self) -> Hsla {
        match self {
            Quadrant::UrgentImportant => rgb(0xff4d4f).into(),
            Quadrant::NotUrgentImportant => rgb(0xfaad14).into(),
            Quadrant::UrgentNotImportant => rgb(0x1890ff).into(),
            Quadrant::NotUrgentNotImportant => rgb(0x52c41a).into(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PriorityFilter {
    All,
    High,
    Medium,
    Low,
}

impl PriorityFilter {
    #[allow(dead_code)]
    fn label(&self) -> &'static str {
        match self {
            PriorityFilter::All => "全部",
            PriorityFilter::High => "高",
            PriorityFilter::Medium => "中",
            PriorityFilter::Low => "低",
        }
    }

    fn matches(&self, priority: Option<Priority>) -> bool {
        match self {
            PriorityFilter::All => true,
            PriorityFilter::High => matches!(priority, Some(Priority::High)),
            PriorityFilter::Medium => matches!(priority, Some(Priority::Medium)),
            PriorityFilter::Low => matches!(priority, Some(Priority::Low) | None),
        }
    }
}

pub struct TaskPanel {
    store: Store,
    tasks: Vec<Record>,
    input_state: Entity<InputState>,
    _subscription: Subscription,
    editing_task_id: Option<uuid::Uuid>,
    task_input_states: HashMap<uuid::Uuid, Entity<InputState>>,
    _edit_subscription: Option<Subscription>,
    context_menu_task_id: Option<uuid::Uuid>,
    context_menu_position: Option<Point<Pixels>>,
    reminder_task_id: Option<uuid::Uuid>,
    reminder_date_picker: Option<Entity<DatePickerState>>,
    reminder_time_input: Option<Entity<InputState>>,
    reminder_error_message: Option<String>,
    _reminder_subscriptions: Vec<Subscription>,
    _window_activation_subscription: Subscription,
    show_completed: bool,
    current_view: TaskView,
    priority_filter: PriorityFilter,
    task_detail_sidebar: Entity<TaskDetailSidebar>,
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
            task_input_states: HashMap::new(),
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
            current_view: TaskView::List,
            priority_filter: PriorityFilter::All,
            task_detail_sidebar: cx.new(|cx| TaskDetailSidebar::new(window, cx)),
        };

        panel.load_tasks(cx);
        panel
    }

    fn categorize_quadrant(task: &Record) -> Quadrant {
        let is_urgent = task.due_date.map_or(false, |d| {
            d.signed_duration_since(Utc::now()).num_hours() < 24
        });
        let is_important = matches!(task.priority, Some(Priority::High));
        
        match (is_urgent, is_important) {
            (true, true) => Quadrant::UrgentImportant,
            (false, true) => Quadrant::NotUrgentImportant,
            (true, false) => Quadrant::UrgentNotImportant,
            (false, false) => Quadrant::NotUrgentNotImportant,
        }
    }

    fn get_filtered_tasks(&self) -> Vec<Record> {
        self.tasks
            .iter()
            .filter(|t| self.priority_filter.matches(t.priority.clone()))
            .filter(|t| t.completed_at.is_none() || self.show_completed)
            .cloned()
            .collect()
    }

    fn group_tasks_by_quadrant(&self) -> HashMap<Quadrant, Vec<Record>> {
        let mut groups: HashMap<Quadrant, Vec<Record>> = HashMap::new();
        
        groups.insert(Quadrant::UrgentImportant, Vec::new());
        groups.insert(Quadrant::NotUrgentImportant, Vec::new());
        groups.insert(Quadrant::UrgentNotImportant, Vec::new());
        groups.insert(Quadrant::NotUrgentNotImportant, Vec::new());
        
        for task in self.get_filtered_tasks().iter().filter(|t| t.completed_at.is_none()) {
            let quadrant = Self::categorize_quadrant(task);
            groups.entry(quadrant).or_default().push(task.clone());
        }
        
        groups
    }

    fn set_view(&mut self, view: TaskView, _window: &mut Window, cx: &mut Context<Self>) {
        self.current_view = view;
        cx.notify();
    }

    fn set_priority_filter(&mut self, filter: PriorityFilter, _window: &mut Window, cx: &mut Context<Self>) {
        self.priority_filter = filter;
        cx.notify();
    }

    fn select_task(&mut self, task: &Record, window: &mut Window, cx: &mut Context<Self>) {
        self.task_detail_sidebar.update(cx, |sidebar, cx| {
            sidebar.show_task(task, window, cx);
        });
        cx.notify();
    }

    fn close_task_detail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.task_detail_sidebar.update(cx, |sidebar, cx| {
            sidebar.close(window, cx);
        });
    }

    fn start_edit(&mut self, task: Record, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_task_id == Some(task.id) {
            return;
        }
        
        self.editing_task_id = Some(task.id);
        let task_id = task.id;
        let content = task.content.clone();
        
        let input_state = self.task_input_states
            .get(&task_id)
            .cloned()
            .unwrap_or_else(|| {
                cx.new(|cx| {
                    let mut state = InputState::new(window, cx);
                    state.set_value(&content, window, cx);
                    state
                })
            });
        
        input_state.update(cx, |state, cx| {
            state.focus(window, cx);
        });
        
        if !self.task_input_states.contains_key(&task_id) {
            self.task_input_states.insert(task_id, input_state.clone());
        }

        let _edit_subscription = cx.subscribe_in(
            &input_state,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => {
                        this.save_edit(window, cx);
                    }
                    InputEvent::Blur => {
                        this.save_edit(window, cx);
                    }
                    _ => {}
                }
            },
        );

        self._edit_subscription = Some(_edit_subscription);
        cx.notify();
    }

    fn save_edit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(task_id) = self.editing_task_id {
            if let Some(input_state) = self.task_input_states.get(&task_id) {
                let new_content = input_state.read(cx).text().to_string();
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
        self.editing_task_id = None;
        self._edit_subscription = None;
        cx.notify();
    }

    fn start_reminder(&mut self, task: &Record, window: &mut Window, cx: &mut Context<Self>) {
        let task_id = task.id;
        self.reminder_task_id = Some(task_id);

        let (init_date, init_time_str) = if let Some(scheduled) = task.scheduled_for {
            let local = scheduled.with_timezone(&chrono::Local);
            (local.naive_local().date(), local.format("%H:%M").to_string())
        } else {
            let now = chrono::Local::now();
            (now.naive_local().date(), now.format("%H:%M").to_string())
        };

        let date_picker = cx.new(|cx| {
            let mut picker = DatePickerState::new(window, cx)
                .date_format("%Y-%m-%d");
            picker.set_date(init_date, window, cx);
            picker
        });

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

            if let Some(naive_date) = date.start() {
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
                    return;
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
                panel.task_input_states.retain(|id, _| {
                    panel.tasks.iter().any(|t| t.id == *id)
                });
                cx.notify();
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
            if task.completed_at.is_some() {
                task.completed_at = None;
                task.status = Some(TaskStatus::Todo);
            } else {
                task.completed_at = Some(chrono::Utc::now());
                task.status = Some(TaskStatus::Done);
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

    fn render_view_switcher(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap(px(4.0))
            .child(
                Button::new("view-list")
                    .child(TaskView::List.label())
                    .when(self.current_view == TaskView::List, |b| {
                        b.with_variant(gpui_component::button::ButtonVariant::Primary)
                    })
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.set_view(TaskView::List, window, cx);
                    }))
            )
            .child(
                Button::new("view-matrix")
                    .child(TaskView::Matrix.label())
                    .when(self.current_view == TaskView::Matrix, |b| {
                        b.with_variant(gpui_component::button::ButtonVariant::Primary)
                    })
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.set_view(TaskView::Matrix, window, cx);
                    }))
            )
    }

    fn render_priority_filter(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap(px(4.0))
            .child(
                Button::new("filter-high")
                    .child("高")
                    .when(self.priority_filter == PriorityFilter::High, |b| {
                        b.with_variant(gpui_component::button::ButtonVariant::Primary)
                    })
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.set_priority_filter(PriorityFilter::High, window, cx);
                    }))
            )
            .child(
                Button::new("filter-medium")
                    .child("中")
                    .when(self.priority_filter == PriorityFilter::Medium, |b| {
                        b.with_variant(gpui_component::button::ButtonVariant::Primary)
                    })
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.set_priority_filter(PriorityFilter::Medium, window, cx);
                    }))
            )
            .child(
                Button::new("filter-low")
                    .child("低")
                    .when(self.priority_filter == PriorityFilter::Low, |b| {
                        b.with_variant(gpui_component::button::ButtonVariant::Primary)
                    })
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.set_priority_filter(PriorityFilter::Low, window, cx);
                    }))
            )
            .child(
                Button::new("filter-all")
                    .child("全部")
                    .when(self.priority_filter == PriorityFilter::All, |b| {
                        b.with_variant(gpui_component::button::ButtonVariant::Primary)
                    })
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.set_priority_filter(PriorityFilter::All, window, cx);
                    }))
            )
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .child(self.render_view_switcher(cx))
            .child(self.render_priority_filter(cx))
    }

    fn render_task_card(&mut self, task: &Record, idx: usize, compact: bool, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let task_id = task.id;
        let is_completed = task.completed_at.is_some();
        let sidebar_task_id = self.task_detail_sidebar.read(cx).current_task_id().map(|s| s.to_string());
        let is_selected = sidebar_task_id == Some(task_id.to_string());
        
        let priority_marker = match task.priority {
            Some(Priority::High) => "!!",
            Some(Priority::Medium) => "!",
            Some(Priority::Low) | None => "",
        };
        let priority_color = match task.priority {
            Some(Priority::High) => rgb(0xff4d4f),
            Some(Priority::Medium) => rgb(0xfaad14),
            Some(Priority::Low) | None => rgb(0x52c41a),
        };

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
            .id(("task-card", idx))
            .w_full()
            .p(px(if compact { 8.0 } else { 12.0 }))
            .rounded(px(6.0))
            .bg(if is_selected { 
                rgb(0xe6f7ff) 
            } else if is_completed { 
                rgb(0xf5f5f5) 
            } else { 
                rgb(0xffffff) 
            })
            .border_1()
            .border_color(if is_selected {
                rgb(0x1890ff)
            } else {
                rgb(0xe8e8e8)
            })
            .hover(|s| s.bg(if is_selected { rgb(0xe6f7ff) } else { rgb(0xf6ffed) }))
            .cursor_pointer()
            .on_click(cx.listener({
                let task = task.clone();
                move |this, _event: &ClickEvent, window, cx| {
                    this.select_task(&task, window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                h_flex()
                    .w_full()
                    .gap(px(8.0))
                    .items_start()
                    .child(
                        div()
                            .id(("checkbox", idx))
                            .cursor_pointer()
                            .child(if is_completed { "☑" } else { "☐" })
                            .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                                this.toggle_task_complete(task_id, cx);
                                cx.stop_propagation();
                            }))
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .gap(px(if compact { 2.0 } else { 4.0 }))
                            .child({
                                let task_id_for_edit = task_id;
                                let task_content = task.content.clone();
                                let input_state = self.task_input_states
                                    .get(&task_id)
                                    .cloned()
                                    .unwrap_or_else(|| {
                                        let state = cx.new(|cx| {
                                            let mut s = InputState::new(window, cx);
                                            s.set_value(&task_content, window, cx);
                                            s
                                        });
                                        
                                        cx.subscribe_in(&state, window, move |this, _state, event: &InputEvent, _window, cx| {
                                            match event {
                                                InputEvent::Blur | InputEvent::PressEnter { .. } => {
                                                    let new_content = this.task_input_states
                                                        .get(&task_id_for_edit)
                                                        .map(|s| s.read(cx).text().to_string())
                                                        .unwrap_or_default();
                                                    
                                                    if let Some(task) = this.tasks.iter_mut().find(|t| t.id == task_id_for_edit) {
                                                        if task.content != new_content && !new_content.trim().is_empty() {
                                                            task.content = new_content;
                                                            task.updated_at = chrono::Utc::now();
                                                            let updated_task = task.clone();
                                                            let store = this.store.clone();
                                                            cx.spawn(async move |_view, _cx| {
                                                                if let Err(e) = store.update_record(updated_task).await {
                                                                    eprintln!("[TaskPanel] Failed to update task: {}", e);
                                                                }
                                                            }).detach();
                                                        }
                                                    }
                                                }
                                                _ => {}
                                            }
                                        }).detach();
                                        
                                        state
                                    });
                                
                                if !self.task_input_states.contains_key(&task_id) {
                                    self.task_input_states.insert(task_id, input_state.clone());
                                }
                                
                                h_flex()
                                    .gap(px(4.0))
                                    .items_center()
                                    .when(!priority_marker.is_empty(), |el| {
                                        el.child(
                                            div()
                                                .text_xs()
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(priority_color)
                                                .child(priority_marker)
                                        )
                                    })
                                    .child(
                                        div()
                                            .flex_1()
                                            .child({
                                                Input::new(&input_state)
                                                    .appearance(false)
                                                    .focus_bordered(false)
                                                    .text_size(px(14.0))
                                                    .text_color(if is_completed { rgb(0x999999) } else { rgb(0x333333) })
                                                    .disabled(true)
                                            })
                                    )
                            })
                            .when(!compact, |el| {
                                el.child(
                                    h_flex()
                                        .gap(px(8.0))
                                        .text_xs()
                                        .text_color(rgb(0xbbbbbb))
                                        .children(task.due_date.map(|t| {
                                            div()
                                                .text_color(rgb(0xff4d4f))
                                                .child(format!("⏰ {}", fmt_short(t)))
                                        }))
                                        .children(task.scheduled_for.map(|t| {
                                            div()
                                                .text_color(rgb(0x1890ff))
                                                .child(format!("📅 {}", fmt_short(t)))
                                        }))
                                )
                            })
                    )
            )
    }

    fn render_matrix_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let groups = self.group_tasks_by_quadrant();
        
        let urgent_important = groups.get(&Quadrant::UrgentImportant).cloned().unwrap_or_default();
        let not_urgent_important = groups.get(&Quadrant::NotUrgentImportant).cloned().unwrap_or_default();
        let urgent_not_important = groups.get(&Quadrant::UrgentNotImportant).cloned().unwrap_or_default();
        let not_urgent_not_important = groups.get(&Quadrant::NotUrgentNotImportant).cloned().unwrap_or_default();

        v_flex()
            .flex_1()
            .gap(px(8.0))
            .child(
                h_flex()
                    .h_1_2()
                    .gap(px(8.0))
                    .child(self.render_quadrant(Quadrant::UrgentImportant, &urgent_important, window, cx))
                    .child(self.render_quadrant(Quadrant::NotUrgentImportant, &not_urgent_important, window, cx))
            )
            .child(
                h_flex()
                    .h_1_2()
                    .gap(px(8.0))
                    .child(self.render_quadrant(Quadrant::UrgentNotImportant, &urgent_not_important, window, cx))
                    .child(self.render_quadrant(Quadrant::NotUrgentNotImportant, &not_urgent_not_important, window, cx))
            )
    }

    fn render_quadrant(&mut self, quadrant: Quadrant, tasks: &[Record], window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let quadrant_color = quadrant.color();
        let task_count = tasks.len();
        let quadrant_idx = match quadrant {
            Quadrant::UrgentImportant => 0,
            Quadrant::NotUrgentImportant => 1,
            Quadrant::UrgentNotImportant => 2,
            Quadrant::NotUrgentNotImportant => 3,
        };
        
        v_flex()
            .flex_1()
            .rounded(px(8.0))
            .border_1()
            .border_color(quadrant_color.opacity(0.3))
            .bg(quadrant_color.opacity(0.05))
            .child(
                div()
                    .p(px(12.0))
                    .border_b_1()
                    .border_color(quadrant_color.opacity(0.2))
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .w(px(8.0))
                                    .h(px(8.0))
                                    .rounded_full()
                                    .bg(quadrant_color)
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(quadrant_color)
                                    .child(quadrant.label())
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x999999))
                                    .child(format!("({})", task_count))
                            )
                    )
            )
            .child(
                div()
                    .flex_1()
                    .p(px(8.0))
                    .overflow_y_scrollbar()
                    .child(
                        v_flex()
                            .gap(px(6.0))
                            .children(tasks.iter().enumerate().map(|(idx, task)| {
                                self.render_task_card(task, quadrant_idx * 1000 + idx, true, window, cx).into_any_element()
                            }))
                    )
            )
    }

    fn render_list_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (pending_tasks, completed_tasks): (Vec<_>, Vec<_>) = self.get_filtered_tasks()
            .iter()
            .cloned()
            .partition(|task| task.completed_at.is_none());

        let pending_count = pending_tasks.len();
        let completed_count = completed_tasks.len();

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

                if pending_count > 0 {
                    elements.push(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0x666666))
                            .child(format!("待办任务 ({})", pending_count))
                            .into_any_element()
                    );
                    for (idx, task) in pending_tasks.iter().enumerate() {
                        elements.push(self.render_task_card(task, idx, false, window, cx).into_any_element());
                    }
                }

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
                        for (idx, task) in completed_tasks.iter().enumerate() {
                            elements.push(self.render_task_card(task, idx, false, window, cx).into_any_element());
                        }
                    }
                }

                elements
            })
    }

}

impl Render for TaskPanel {
    #[allow(unused_variables)]
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (pending_count, completed_count): (usize, usize) = self.tasks.iter()
            .fold((0, 0), |(p, c), task| {
                if task.completed_at.is_some() {
                    (p, c + 1)
                } else {
                    (p + 1, c)
                }
            });

        div()
            .size_full()
            .flex()
            .flex_row()
            .relative()
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
                    .id("task-panel-main")
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .p(px(16.0))
                    .on_click(cx.listener(|this, _event: &ClickEvent, window, cx| {
                        if this.task_detail_sidebar.read(cx).current_task_id().is_some() {
                            this.close_task_detail(window, cx);
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
                    .child(self.render_toolbar(cx))
                    .when(self.reminder_task_id.is_some(), |el| {
                        if let (Some(ref dp), Some(ref ti)) = (
                            &self.reminder_date_picker, 
                            &self.reminder_time_input
                        ) {
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
                    .child(
                        div()
                            .id("task-view-container")
                            .flex_1()
                            .overflow_hidden()
                            .child(
                                match self.current_view {
                                    TaskView::List => self.render_list_view(window, cx).into_any_element(),
                                    TaskView::Matrix => self.render_matrix_view(window, cx).into_any_element(),
                                }
                            )
                    )
                    .children(self.render_custom_context_menu(cx))
            )
            .child(self.task_detail_sidebar.clone())
    }
}
