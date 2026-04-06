use crate::models::{Priority, Record, TaskStatus};
use crate::store::Store;
use crate::ui::parsing;
use crate::ui::sidebar::{main_sidebar_layout_mode, main_sidebar_width};
use crate::ui::task_detail_sidebar::TaskDetailSidebar;
use chrono::{Datelike, Duration, Local, TimeZone, Timelike, Utc};
use gpui::prelude::FluentBuilder as _;
use gpui::StatefulInteractiveElement as _;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::date_picker::{DatePicker, DatePickerState};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::{h_flex, v_flex, IconName};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

actions!(
    task_panel,
    [
        EditTaskAction,
        DeleteTaskAction,
        SetReminderAction,
        SetReminderTodayAction,
        SetReminderTomorrowAction,
        SetReminderNextWeekAction
    ]
);

const TASK_DETAIL_SIDEBAR_WIDTH: Pixels = px(360.0);
const TASK_PANEL_HORIZONTAL_PADDING: Pixels = px(32.0);
const MATRIX_COLUMN_GAP: Pixels = px(8.0);
const MIN_VISIBLE_QUADRANT_WIDTH: Pixels = px(280.0);
const TASK_CARD_TITLE_LIMIT: usize = 24;
const TASK_CARD_PREVIEW_LIMIT: usize = 44;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MatrixLayoutMode {
    Grid,
    Stacked,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PriorityFilter {
    All,
    High,
    Medium,
    Low,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TagFilterMode {
    And,
    Or,
}

impl TagFilterMode {
    fn label(&self) -> &'static str {
        match self {
            TagFilterMode::And => "AND",
            TagFilterMode::Or => "OR",
        }
    }

    fn toggle(&self) -> Self {
        match self {
            TagFilterMode::And => TagFilterMode::Or,
            TagFilterMode::Or => TagFilterMode::And,
        }
    }
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

#[derive(Clone)]
struct PendingDeletion {
    id: Uuid,
    record_label: &'static str,
    display_title: String,
}

pub struct TaskPanel {
    store: Store,
    tasks: Vec<Record>,
    focus_handle: FocusHandle,
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
    _window_bounds_subscription: Subscription,
    show_completed: bool,
    current_view: TaskView,
    matrix_layout_mode: MatrixLayoutMode,
    matrix_stack_scroll_handle: ScrollHandle,
    pending_stack_scroll_target: Option<usize>,
    priority_filter: PriorityFilter,
    selected_tags: HashSet<String>,
    available_tags: Vec<String>,
    tag_filter_mode: TagFilterMode,
    pending_deletion: Option<PendingDeletion>,
    task_detail_sidebar: Entity<TaskDetailSidebar>,
}

impl TaskPanel {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("!! 高优先级 | ! 普通优先级 | 直接输入 | #标签 @人物")
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

        let _window_activation_subscription =
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.load_tasks(cx);
                    this.sync_matrix_layout_mode(window, cx, false);
                }
            });
        let _window_bounds_subscription = cx.observe_window_bounds(window, |this, window, cx| {
            this.sync_matrix_layout_mode(window, cx, false);
        });

        let mut panel = Self {
            store,
            tasks: Vec::new(),
            focus_handle,
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
            _window_activation_subscription,
            _window_bounds_subscription,
            show_completed: false,
            current_view: TaskView::List,
            matrix_layout_mode: MatrixLayoutMode::Grid,
            matrix_stack_scroll_handle: ScrollHandle::new(),
            pending_stack_scroll_target: None,
            priority_filter: PriorityFilter::All,
            selected_tags: HashSet::new(),
            available_tags: Vec::new(),
            tag_filter_mode: TagFilterMode::And,
            pending_deletion: None,
            task_detail_sidebar: cx.new(|cx| TaskDetailSidebar::new(window, cx)),
        };

        let handle = cx.entity().clone();
        panel.task_detail_sidebar.update(cx, |sidebar, _cx| {
            sidebar.on_save(move |payload, cx| {
                handle.update(cx, |panel, cx| {
                    panel.handle_sidebar_save(&payload, cx);
                });
            });
        });
        let handle = cx.entity().clone();
        panel.task_detail_sidebar.update(cx, |sidebar, _cx| {
            sidebar.on_delete(move |task_id, cx| {
                if let Ok(task_id) = Uuid::parse_str(&task_id) {
                    handle.update(cx, |panel, cx| {
                        panel.request_delete_task(task_id, cx);
                    });
                }
            });
        });
        let handle = cx.entity().clone();
        panel.task_detail_sidebar.update(cx, |sidebar, _cx| {
            sidebar.on_close(move |cx| {
                handle.update(cx, |panel, cx| {
                    panel.handle_sidebar_close(cx);
                });
            });
        });

        panel.load_tasks(cx);
        panel.load_available_tags(cx);
        panel
    }

    pub fn focus_primary_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_deletion.is_some()
            || self.reminder_task_id.is_some()
            || self.sidebar_visible(cx)
            || self.editing_task_id.is_some()
        {
            self.focus_handle.focus(window, cx);
            return;
        }

        self.focus_handle.focus(window, cx);
        self.input_state.update(cx, |state, cx| {
            state.focus(window, cx);
        });
    }

    fn handle_sidebar_save(
        &mut self,
        payload: &crate::ui::task_detail_sidebar::SavePayload,
        cx: &mut Context<Self>,
    ) {
        if let Some(task) = self
            .tasks
            .iter_mut()
            .find(|t| t.id.to_string() == payload.task_id)
        {
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
                TaskStatus::Done => {
                    if task.completed_at.is_none() {
                        task.completed_at = Some(chrono::Utc::now());
                    }
                }
                TaskStatus::Cancelled => {
                    if task.completed_at.is_none() {
                        task.completed_at = Some(chrono::Utc::now());
                    }
                }
                _ => {
                    task.completed_at = None;
                }
            }

            let updated_task = task.clone();
            let store = self.store.clone();
            cx.spawn(
                async move |view, cx| match store.update_record(updated_task).await {
                    Ok(_) => {
                        view.update(cx, |panel, cx| {
                            panel.load_available_tags(cx);
                        })
                        .ok();
                    }
                    Err(e) => {
                        eprintln!("[TaskPanel] Failed to update task: {}", e);
                    }
                },
            )
            .detach();

            cx.notify();
        }
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
            .filter(|t| self.matches_tag_filter(t))
            .cloned()
            .collect()
    }

    fn matches_tag_filter(&self, task: &Record) -> bool {
        if self.selected_tags.is_empty() {
            return true;
        }

        let task_tags: HashSet<&String> = task.tags.iter().collect();

        match self.tag_filter_mode {
            TagFilterMode::And => {
                // AND 模式：任务必须包含所有选中的标签
                self.selected_tags.iter().all(|tag| task_tags.contains(tag))
            }
            TagFilterMode::Or => {
                // OR 模式：任务只需包含任意一个选中的标签
                self.selected_tags.iter().any(|tag| task_tags.contains(tag))
            }
        }
    }

    fn group_tasks_by_quadrant(&self) -> HashMap<Quadrant, Vec<Record>> {
        let mut groups: HashMap<Quadrant, Vec<Record>> = HashMap::new();

        groups.insert(Quadrant::UrgentImportant, Vec::new());
        groups.insert(Quadrant::NotUrgentImportant, Vec::new());
        groups.insert(Quadrant::UrgentNotImportant, Vec::new());
        groups.insert(Quadrant::NotUrgentNotImportant, Vec::new());

        for task in self
            .get_filtered_tasks()
            .iter()
            .filter(|t| t.completed_at.is_none())
        {
            let quadrant = Self::categorize_quadrant(task);
            groups.entry(quadrant).or_default().push(task.clone());
        }

        groups
    }

    fn set_view(&mut self, view: TaskView, window: &mut Window, cx: &mut Context<Self>) {
        self.current_view = view;
        if view != TaskView::Matrix {
            self.matrix_layout_mode = MatrixLayoutMode::Grid;
            self.pending_stack_scroll_target = None;
        } else {
            self.sync_matrix_layout_mode(window, cx, true);
            return;
        }
        cx.notify();
    }

    fn set_priority_filter(
        &mut self,
        filter: PriorityFilter,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.priority_filter = filter;
        cx.notify();
    }

    fn select_task(&mut self, task: &Record, window: &mut Window, cx: &mut Context<Self>) {
        self.task_detail_sidebar.update(cx, |sidebar, cx| {
            sidebar.show_task(task, window, cx);
        });
        self.sync_matrix_layout_mode(window, cx, true);
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

    fn task_display_name(task: &Record) -> String {
        let base = task
            .title
            .clone()
            .filter(|title| !title.trim().is_empty())
            .or_else(|| {
                task.content
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .map(|line| line.trim().to_string())
            })
            .unwrap_or_else(|| "无标题任务".to_string());
        Self::truncate_text(&base, TASK_CARD_TITLE_LIMIT)
    }

    fn task_preview(task: &Record) -> String {
        let normalized_content = Self::normalize_text(&task.content);
        if normalized_content.is_empty() {
            return String::new();
        }

        let normalized_title = task
            .title
            .as_deref()
            .map(Self::normalize_text)
            .unwrap_or_default();

        if !normalized_title.is_empty() && normalized_content == normalized_title {
            return String::new();
        }

        Self::truncate_text(&normalized_content, TASK_CARD_PREVIEW_LIMIT)
    }

    fn handle_sidebar_close(&mut self, cx: &mut Context<Self>) {
        self.matrix_layout_mode = MatrixLayoutMode::Grid;
        self.pending_stack_scroll_target = None;
        cx.notify();
    }

    fn sidebar_visible(&self, cx: &App) -> bool {
        self.task_detail_sidebar
            .read(cx)
            .current_task_id()
            .is_some()
    }

    fn selected_task_quadrant(&self, cx: &App) -> Option<Quadrant> {
        let task_id = self
            .task_detail_sidebar
            .read(cx)
            .current_task_id()?
            .to_string();
        self.tasks
            .iter()
            .find(|task| task.id.to_string() == task_id)
            .map(Self::categorize_quadrant)
    }

    fn quadrant_stack_index(quadrant: Quadrant) -> usize {
        match quadrant {
            Quadrant::UrgentImportant => 0,
            Quadrant::NotUrgentImportant => 1,
            Quadrant::UrgentNotImportant => 2,
            Quadrant::NotUrgentNotImportant => 3,
        }
    }

    fn selected_quadrant_visible_width(&self, quadrant: Quadrant, window: &Window) -> Pixels {
        let viewport_width = window.viewport_size().width;
        let sidebar_width = main_sidebar_width(main_sidebar_layout_mode(viewport_width));
        let task_panel_width = std::cmp::max(viewport_width - sidebar_width, px(0.0));
        let matrix_content_width =
            std::cmp::max(task_panel_width - TASK_PANEL_HORIZONTAL_PADDING, px(0.0));
        let column_width = std::cmp::max((matrix_content_width - MATRIX_COLUMN_GAP) * 0.5, px(0.0));

        match quadrant {
            Quadrant::NotUrgentImportant | Quadrant::NotUrgentNotImportant => {
                std::cmp::max(column_width - TASK_DETAIL_SIDEBAR_WIDTH, px(0.0))
            }
            Quadrant::UrgentImportant | Quadrant::UrgentNotImportant => column_width,
        }
    }

    fn current_matrix_layout_mode(&self, window: &Window, cx: &App) -> MatrixLayoutMode {
        if self.current_view != TaskView::Matrix || !self.sidebar_visible(cx) {
            return MatrixLayoutMode::Grid;
        }

        let Some(quadrant) = self.selected_task_quadrant(cx) else {
            return MatrixLayoutMode::Grid;
        };

        if self.selected_quadrant_visible_width(quadrant, window) < MIN_VISIBLE_QUADRANT_WIDTH {
            MatrixLayoutMode::Stacked
        } else {
            MatrixLayoutMode::Grid
        }
    }

    fn sync_matrix_layout_mode(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
        force_scroll_to_selected: bool,
    ) {
        let next_mode = self.current_matrix_layout_mode(window, cx);
        let previous_mode = self.matrix_layout_mode;

        if next_mode == MatrixLayoutMode::Stacked
            && (force_scroll_to_selected || next_mode != previous_mode)
        {
            self.pending_stack_scroll_target = self
                .selected_task_quadrant(cx)
                .map(Self::quadrant_stack_index);
        } else if next_mode == MatrixLayoutMode::Grid {
            self.pending_stack_scroll_target = None;
        }

        self.matrix_layout_mode = next_mode;

        if previous_mode != next_mode || force_scroll_to_selected {
            cx.notify();
        }
    }

    fn start_edit(&mut self, task: Record, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_task_id == Some(task.id) {
            return;
        }

        self.editing_task_id = Some(task.id);
        let task_id = task.id;
        let content = task.content.clone();

        let input_state = self
            .task_input_states
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
            |this, _state, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    this.save_edit(window, cx);
                }
                InputEvent::Blur => {
                    this.save_edit(window, cx);
                }
                _ => {}
            },
        );

        self._edit_subscription = Some(_edit_subscription);
        cx.notify();
    }

    fn save_edit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(task_id) = self.editing_task_id {
            if let Some(input_state) = self.task_input_states.get(&task_id) {
                let new_title = input_state.read(cx).text().to_string();
                let (title, priority, tags, people) = parsing::parse_task_input(&new_title);

                if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    task.title = Some(title);
                    task.priority = Some(priority);
                    task.tags = tags;
                    task.persons = people;
                    task.updated_at = chrono::Utc::now();
                    let updated_task = task.clone();
                    let store = self.store.clone();
                    cx.spawn(
                        async move |view, cx| match store.update_record(updated_task).await {
                            Ok(_) => {
                                view.update(cx, |panel, cx| {
                                    panel.load_available_tags(cx);
                                })
                                .ok();
                            }
                            Err(e) => {
                                eprintln!("[TaskPanel] Failed to update task: {}", e);
                            }
                        },
                    )
                    .detach();
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
            (
                local.naive_local().date(),
                local.format("%H:%M").to_string(),
            )
        } else {
            let now = chrono::Local::now();
            (now.naive_local().date(), now.format("%H:%M").to_string())
        };

        let date_picker = cx.new(|cx| {
            let mut picker = DatePickerState::new(window, cx).date_format("%Y-%m-%d");
            picker.set_date(init_date, window, cx);
            picker
        });

        let time_input = cx.new(|cx| InputState::new(window, cx).placeholder("HH:MM 如 14:30"));
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
                        if let Some(local_dt) =
                            chrono::Local.from_local_datetime(&naive_dt).single()
                        {
                            let utc_dt = local_dt.with_timezone(&chrono::Utc);

                            if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                                task.scheduled_for = Some(utc_dt);
                                task.notified_at = None;
                                let updated_task = task.clone();
                                let store = self.store.clone();
                                cx.spawn(async move |_view, _cx| {
                                    if let Err(e) = store.update_record(updated_task).await {
                                        eprintln!(
                                            "[TaskPanel] Failed to update task reminder: {}",
                                            e
                                        );
                                    }
                                })
                                .detach();
                            }
                        }
                    }
                } else {
                    self.reminder_error_message =
                        Some("时间格式无效，请输入 HH:MM（例 14:30）".to_string());
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

    fn update_task_reminder(
        &mut self,
        task_id: Uuid,
        scheduled_for: chrono::DateTime<chrono::Utc>,
        cx: &mut Context<Self>,
    ) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.scheduled_for = Some(scheduled_for);
            let updated_task = task.clone();
            let store = self.store.clone();
            cx.spawn(async move |_view, _cx| {
                if let Err(e) = store.update_record(updated_task).await {
                    eprintln!("[TaskPanel] Failed to update reminder: {}", e);
                }
            })
            .detach();
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
                eprintln!(
                    "[TaskPanel] Fetching tasks from store... (attempt {})",
                    retries + 1
                );
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
                panel
                    .task_input_states
                    .retain(|id, _| panel.tasks.iter().any(|t| t.id == *id));
                cx.notify();
                eprintln!(
                    "[TaskPanel] Tasks updated and notified, panel now has {} tasks",
                    panel.tasks.len()
                );
            });
            if let Err(e) = update_result {
                eprintln!("[TaskPanel] Failed to update view: {:?}", e);
            }
        })
        .detach();
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
                eprintln!("[TaskPanel] Failed to load tags: {}", e);
            }
        })
        .detach();
    }

    fn toggle_tag_filter(&mut self, tag: &str, cx: &mut Context<Self>) {
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

    fn toggle_tag_filter_mode(&mut self, cx: &mut Context<Self>) {
        self.tag_filter_mode = self.tag_filter_mode.toggle();
        cx.notify();
    }

    fn create_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input_state.read(cx).text().to_string();
        eprintln!("[TaskPanel] create_task called with text: '{}'", text);
        if text.trim().is_empty() {
            eprintln!("[TaskPanel] Text is empty, returning");
            return;
        }

        let (title, priority, tags, people) = parsing::parse_task_input(&text);
        eprintln!(
            "[TaskPanel] Parsed title: '{}', priority: {:?}, tags: {:?}, people: {:?}",
            title, priority, tags, people
        );

        // 任务创建时，输入内容作为 title，content 初始为空
        let mut task = Record::new_task(title, String::new(), priority);
        task.tags = tags;
        task.persons = people;
        eprintln!(
            "[TaskPanel] Created task with id: {}, tags: {:?}, persons: {:?}",
            task.id, task.tags, task.persons
        );

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
                        panel.load_available_tags(cx);
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
        })
        .detach();
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

            cx.spawn(
                async move |view, cx| match store.update_record(updated_task).await {
                    Ok(_) => {
                        view.update(cx, |panel, cx| {
                            panel.load_tasks(cx);
                        })
                        .ok();
                    }
                    Err(e) => eprintln!("Failed to toggle task: {}", e),
                },
            )
            .detach();
        }
    }

    fn request_delete_task(&mut self, task_id: Uuid, cx: &mut Context<Self>) {
        if let Some(task) = self.tasks.iter().find(|t| t.id == task_id) {
            self.context_menu_task_id = None;
            self.context_menu_position = None;
            self.pending_deletion = Some(PendingDeletion {
                id: task_id,
                record_label: "任务",
                display_title: Self::task_display_name(task),
            });
            cx.notify();
        }
    }

    fn cancel_delete_confirmation(&mut self, cx: &mut Context<Self>) {
        self.pending_deletion = None;
        cx.notify();
    }

    fn confirm_delete_task(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_deletion.clone() else {
            return;
        };

        self.perform_delete_task(pending.id, cx);
    }

    fn perform_delete_task(&mut self, task_id: Uuid, cx: &mut Context<Self>) {
        let store = self.store.clone();
        let task_id_string = task_id.to_string();

        cx.spawn(
            async move |view, cx| match store.delete_record(task_id).await {
                Ok(_) => {
                    view.update(cx, |panel, cx| {
                        panel.pending_deletion = None;
                        if panel.task_detail_sidebar.read(cx).current_task_id()
                            == Some(task_id_string.as_str())
                        {
                            panel.task_detail_sidebar.update(cx, |sidebar, cx| {
                                sidebar.dismiss(cx);
                            });
                            panel.handle_sidebar_close(cx);
                        }
                        panel.load_tasks(cx);
                        panel.load_available_tags(cx);
                    })
                    .ok();
                }
                Err(e) => eprintln!("Failed to delete task: {}", e),
            },
        )
        .detach();
    }

    fn render_delete_confirmation(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pending = self.pending_deletion.as_ref()?;
        let title = pending.display_title.clone();
        let record_label = pending.record_label;

        Some(
            div()
                .id("task-delete-confirm-overlay")
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
                            h_flex()
                                .justify_end()
                                .gap(px(8.0))
                                .child(
                                    Button::new("task-delete-confirm-cancel")
                                        .child("取消")
                                        .on_click(cx.listener(|this, _event, _window, cx| {
                                            this.cancel_delete_confirmation(cx);
                                        })),
                                )
                                .child(
                                    Button::new("task-delete-confirm-submit")
                                        .child("确认删除")
                                        .text_color(rgb(0xff4d4f))
                                        .on_click(cx.listener(|this, _event, _window, cx| {
                                            this.confirm_delete_task(cx);
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_custom_context_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (_task_id, position) = match (self.context_menu_task_id, self.context_menu_position) {
            (Some(id), Some(pos)) => (id, pos),
            _ => return None,
        };

        Some(
            deferred(
                anchored().position(position).child(
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
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(0xaaaaaa))
                                        .mb_1()
                                        .child("日期"),
                                )
                                .child(
                                    h_flex()
                                        .justify_between()
                                        .child(self.render_menu_shortcut(
                                            IconName::Sun,
                                            SetReminderTodayAction,
                                            cx,
                                        ))
                                        .child(self.render_menu_shortcut(
                                            IconName::Bell,
                                            SetReminderTomorrowAction,
                                            cx,
                                        ))
                                        .child(self.render_menu_shortcut(
                                            IconName::Calendar,
                                            SetReminderNextWeekAction,
                                            cx,
                                        ))
                                        .child(self.render_menu_shortcut(
                                            IconName::Plus,
                                            SetReminderAction,
                                            cx,
                                        )),
                                ),
                        )
                        .child(div().h_px().bg(rgb(0xeeeeee)))
                        .child(self.render_menu_item(
                            "设置提醒",
                            IconName::Bell,
                            SetReminderAction,
                            cx,
                        ))
                        .child(self.render_menu_item(
                            "编辑",
                            IconName::Settings,
                            EditTaskAction,
                            cx,
                        ))
                        .child(div().h_px().bg(rgb(0xeeeeee)))
                        .child(self.render_menu_item(
                            "删除",
                            IconName::Delete,
                            DeleteTaskAction,
                            cx,
                        )),
                ),
            )
            .into_any(),
        )
    }

    fn render_menu_shortcut<A: Action + 'static>(
        &self,
        icon: IconName,
        action: A,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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
                    .text_color(rgb(0x666666)),
            )
    }

    fn render_menu_item<A: Action + 'static>(
        &self,
        label: &'static str,
        icon: IconName,
        action: A,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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
                    .text_color(rgb(0x666666)),
            )
            .child(div().text_sm().text_color(rgb(0x333333)).child(label))
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
                    })),
            )
            .child(
                Button::new("view-matrix")
                    .child(TaskView::Matrix.label())
                    .when(self.current_view == TaskView::Matrix, |b| {
                        b.with_variant(gpui_component::button::ButtonVariant::Primary)
                    })
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.set_view(TaskView::Matrix, window, cx);
                    })),
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
                    })),
            )
            .child(
                Button::new("filter-medium")
                    .child("中")
                    .when(self.priority_filter == PriorityFilter::Medium, |b| {
                        b.with_variant(gpui_component::button::ButtonVariant::Primary)
                    })
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.set_priority_filter(PriorityFilter::Medium, window, cx);
                    })),
            )
            .child(
                Button::new("filter-low")
                    .child("低")
                    .when(self.priority_filter == PriorityFilter::Low, |b| {
                        b.with_variant(gpui_component::button::ButtonVariant::Primary)
                    })
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.set_priority_filter(PriorityFilter::Low, window, cx);
                    })),
            )
            .child(
                Button::new("filter-all")
                    .child("全部")
                    .when(self.priority_filter == PriorityFilter::All, |b| {
                        b.with_variant(gpui_component::button::ButtonVariant::Primary)
                    })
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.set_priority_filter(PriorityFilter::All, window, cx);
                    })),
            )
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .gap(px(8.0))
            .child(self.render_view_switcher(cx))
            .child(self.render_priority_filter(cx))
    }

    fn render_tag_filter(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_selected = !self.selected_tags.is_empty();
        let mode_label = self.tag_filter_mode.label();

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
                                .id(("tag-filter", idx))
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
                                        this.toggle_tag_filter(&tag_clone, cx);
                                    },
                                ))
                        }))
                        .when(has_selected, |el| {
                            el.child(
                                h_flex()
                                    .gap(px(8.0))
                                    .items_center()
                                    .child(
                                        Button::new("toggle-mode")
                                            .child(mode_label.to_string())
                                            .on_click(cx.listener(|this, _event, _window, cx| {
                                                this.toggle_tag_filter_mode(cx);
                                            })),
                                    )
                                    .child(Button::new("clear-tags").child("清除").on_click(
                                        cx.listener(|this, _event, _window, cx| {
                                            this.clear_tag_filters(cx);
                                        }),
                                    )),
                            )
                        }),
                )
            })
    }

    fn render_task_card(
        &mut self,
        task: &Record,
        idx: usize,
        compact: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let task_id = task.id;
        let is_completed = task.completed_at.is_some();
        let sidebar_task_id = self
            .task_detail_sidebar
            .read(cx)
            .current_task_id()
            .map(|s| s.to_string());
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
                format!(
                    "{}月{}日 {}",
                    local.month(),
                    local.day(),
                    local.format("%H:%M")
                )
            } else {
                local.format("%Y-%m-%d %H:%M").to_string()
            }
        };

        // 任务显示标题，如有详细内容则显示预览
        let display_title = Self::task_display_name(task);
        let content_preview = Self::task_preview(task);
        let has_content_preview = !content_preview.is_empty();
        let has_metadata = task.due_date.is_some()
            || task.scheduled_for.is_some()
            || !task.tags.is_empty()
            || !task.persons.is_empty();

        div()
            .id(("task-card", idx))
            .w_full()
            .min_w(px(0.0))
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
                    .min_w(px(0.0))
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
                            .min_w(px(0.0))
                            .gap(px(4.0))
                            .items_start()
                            .overflow_hidden()
                            .child({
                                let is_editing = self.editing_task_id == Some(task_id);
                                let title_element = if is_editing {
                                    let task_title = display_title.clone();
                                    let input_state = self
                                        .task_input_states
                                        .get(&task_id)
                                        .cloned()
                                        .unwrap_or_else(|| {
                                            let state = cx.new(|cx| {
                                                let mut s = InputState::new(window, cx);
                                                s.set_value(&task_title, window, cx);
                                                s
                                            });

                                            cx.subscribe_in(
                                                &state,
                                                window,
                                                move |this, _state, event: &InputEvent, _window, cx| {
                                                    match event {
                                                        InputEvent::Blur | InputEvent::PressEnter { .. } => {
                                                            this.save_edit(_window, cx);
                                                        }
                                                        _ => {}
                                                    }
                                                },
                                            )
                                            .detach();

                                            state
                                        });

                                    if !self.task_input_states.contains_key(&task_id) {
                                        self.task_input_states.insert(task_id, input_state.clone());
                                    }

                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .overflow_hidden()
                                        .child(
                                            Input::new(&input_state)
                                                .flex_1()
                                                .appearance(false)
                                                .focus_bordered(false)
                                                .text_size(px(14.0))
                                                .text_color(if is_completed {
                                                    rgb(0x999999)
                                                } else {
                                                    rgb(0x333333)
                                                }),
                                        )
                                        .into_any_element()
                                } else {
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .overflow_hidden()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(if is_completed {
                                            rgb(0x999999)
                                        } else {
                                            rgb(0x333333)
                                        })
                                        .child(display_title.clone())
                                        .into_any_element()
                                };

                                v_flex()
                                    .w_full()
                                    .min_w(px(0.0))
                                    .gap(px(4.0))
                                    .child(
                                        h_flex()
                                            .w_full()
                                            .min_w(px(0.0))
                                            .gap(px(4.0))
                                            .items_center()
                                            .overflow_hidden()
                                            .when(!priority_marker.is_empty(), |el| {
                                                el.child(
                                                    div()
                                                        .text_xs()
                                                        .font_weight(FontWeight::BOLD)
                                                        .text_color(priority_color)
                                                        .child(priority_marker)
                                                )
                                            })
                                            .child(title_element)
                                    )
                                    .when(has_content_preview && !compact, |el| {
                                        el.child(
                                            div()
                                                .min_w(px(0.0))
                                                .text_sm()
                                                .text_color(rgb(0x888888))
                                                .line_height(relative(1.35))
                                                .child(content_preview)
                                        )
                                    })
                            })
                            .when(has_metadata, |el| {
                                el.child(
                                    div()
                                        .flex()
                                        .min_w(px(0.0))
                                        .gap(px(8.0))
                                        .flex_wrap()
                                        .text_xs()
                                        .text_color(rgb(0xbbbbbb))
                                        .children(task.due_date.map(|t| {
                                            div()
                                                .text_xs()
                                                .text_color(rgb(0xff4d4f))
                                                .child(format!("⏰ {}", fmt_short(t)))
                                        }))
                                        .children(task.scheduled_for.map(|t| {
                                            div()
                                                .text_xs()
                                                .text_color(rgb(0x1890ff))
                                                .child(format!("📅 {}", fmt_short(t)))
                                        }))
                                        .children(task.tags.iter().enumerate().map(|(idx, tag)| {
                                            div()
                                                .id(("task-tag", idx))
                                                .px(px(5.0))
                                                .py(px(1.0))
                                                .rounded(px(4.0))
                                                .bg(rgb(0xf5f5f5))
                                                .text_xs()
                                                .text_color(rgb(0x595959))
                                                .child(format!("#{}", tag))
                                        }))
                                        .children(task.persons.iter().enumerate().map(|(idx, person)| {
                                            div()
                                                .id(("task-person", idx))
                                                .px(px(5.0))
                                                .py(px(1.0))
                                                .rounded(px(4.0))
                                                .bg(rgb(0xe6f7ff))
                                                .text_xs()
                                                .text_color(rgb(0x1890ff))
                                                .child(format!("@{}", person))
                                        }))
                                )
                            })
                    )
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .justify_end()
                            .child(
                                div()
                                    .cursor_pointer()
                                    .px(px(4.0))
                                    .text_color(rgb(0x888888))
                                    .hover(|style| style.text_color(rgb(0xff4d4f)))
                                    .child("×")
                                    .id(("task-delete", idx))
                                    .on_click(cx.listener(
                                        move |this, _event: &ClickEvent, _window, cx| {
                                            this.request_delete_task(task_id, cx);
                                            cx.stop_propagation();
                                        },
                                    )),
                            ),
                    )
            )
    }

    fn render_matrix_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let groups = self.group_tasks_by_quadrant();

        let urgent_important = groups
            .get(&Quadrant::UrgentImportant)
            .cloned()
            .unwrap_or_default();
        let not_urgent_important = groups
            .get(&Quadrant::NotUrgentImportant)
            .cloned()
            .unwrap_or_default();
        let urgent_not_important = groups
            .get(&Quadrant::UrgentNotImportant)
            .cloned()
            .unwrap_or_default();
        let not_urgent_not_important = groups
            .get(&Quadrant::NotUrgentNotImportant)
            .cloned()
            .unwrap_or_default();

        let layout_mode = self.current_matrix_layout_mode(window, cx);
        self.matrix_layout_mode = layout_mode;

        match layout_mode {
            MatrixLayoutMode::Grid => v_flex()
                .flex_1()
                .gap(px(8.0))
                .child(
                    h_flex()
                        .h_1_2()
                        .gap(px(8.0))
                        .child(self.render_quadrant(
                            Quadrant::UrgentImportant,
                            &urgent_important,
                            window,
                            cx,
                        ))
                        .child(self.render_quadrant(
                            Quadrant::NotUrgentImportant,
                            &not_urgent_important,
                            window,
                            cx,
                        )),
                )
                .child(
                    h_flex()
                        .h_1_2()
                        .gap(px(8.0))
                        .child(self.render_quadrant(
                            Quadrant::UrgentNotImportant,
                            &urgent_not_important,
                            window,
                            cx,
                        ))
                        .child(self.render_quadrant(
                            Quadrant::NotUrgentNotImportant,
                            &not_urgent_not_important,
                            window,
                            cx,
                        )),
                )
                .into_any_element(),
            MatrixLayoutMode::Stacked => {
                if let Some(target) = self.pending_stack_scroll_target.take() {
                    self.matrix_stack_scroll_handle
                        .scroll_to_top_of_item(target);
                }

                div()
                    .id("matrix-stacked-scroll")
                    .size_full()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .track_scroll(&self.matrix_stack_scroll_handle)
                    .overflow_y_scroll()
                    .vertical_scrollbar(&self.matrix_stack_scroll_handle)
                    .child(self.render_stacked_quadrant(
                        Quadrant::UrgentImportant,
                        &urgent_important,
                        window,
                        cx,
                    ))
                    .child(self.render_stacked_quadrant(
                        Quadrant::NotUrgentImportant,
                        &not_urgent_important,
                        window,
                        cx,
                    ))
                    .child(self.render_stacked_quadrant(
                        Quadrant::UrgentNotImportant,
                        &urgent_not_important,
                        window,
                        cx,
                    ))
                    .child(self.render_stacked_quadrant(
                        Quadrant::NotUrgentNotImportant,
                        &not_urgent_not_important,
                        window,
                        cx,
                    ))
                    .into_any_element()
            }
        }
    }

    fn render_quadrant(
        &mut self,
        quadrant: Quadrant,
        tasks: &[Record],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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
                                    .bg(quadrant_color),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(quadrant_color)
                                    .child(quadrant.label()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x999999))
                                    .child(format!("({})", task_count)),
                            ),
                    ),
            )
            .child(
                div().flex_1().p(px(8.0)).overflow_y_scrollbar().child(
                    v_flex()
                        .gap(px(6.0))
                        .children(tasks.iter().enumerate().map(|(idx, task)| {
                            self.render_task_card(task, quadrant_idx * 1000 + idx, true, window, cx)
                                .into_any_element()
                        })),
                ),
            )
    }

    fn render_stacked_quadrant(
        &mut self,
        quadrant: Quadrant,
        tasks: &[Record],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let quadrant_color = quadrant.color();
        let task_count = tasks.len();
        let quadrant_idx = match quadrant {
            Quadrant::UrgentImportant => 0,
            Quadrant::NotUrgentImportant => 1,
            Quadrant::UrgentNotImportant => 2,
            Quadrant::NotUrgentNotImportant => 3,
        };

        v_flex()
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
                                    .bg(quadrant_color),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(quadrant_color)
                                    .child(quadrant.label()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x999999))
                                    .child(format!("({})", task_count)),
                            ),
                    ),
            )
            .child(
                div()
                    .p(px(8.0))
                    .min_h(px(if tasks.is_empty() { 72.0 } else { 0.0 }))
                    .child(v_flex().gap(px(6.0)).children(tasks.iter().enumerate().map(
                        |(idx, task)| {
                            self.render_task_card(task, quadrant_idx * 1000 + idx, true, window, cx)
                                .into_any_element()
                        },
                    ))),
            )
    }

    fn render_list_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let filtered_tasks: Vec<_> = self
            .tasks
            .iter()
            .filter(|t| self.priority_filter.matches(t.priority.clone()))
            .cloned()
            .collect();
        let (pending_tasks, completed_tasks): (Vec<_>, Vec<_>) = filtered_tasks
            .iter()
            .cloned()
            .partition(|task| task.completed_at.is_none());

        let pending_count = pending_tasks.len();
        let completed_count = completed_tasks.len();

        div()
            .id("task-list")
            .size_full()
            .min_w(px(0.0))
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
                            .into_any_element(),
                    );
                    for (idx, task) in pending_tasks.iter().enumerate() {
                        elements.push(
                            self.render_task_card(task, idx, false, window, cx)
                                .into_any_element(),
                        );
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
                                div().flex().items_center().gap(px(4.0)).child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(0x999999))
                                        .child(format!(
                                            "{} 已完成 ({})",
                                            if self.show_completed { "▼" } else { "▶" },
                                            completed_count
                                        )),
                                ),
                            )
                            .into_any_element(),
                    );

                    if self.show_completed {
                        for (idx, task) in completed_tasks.iter().enumerate() {
                            elements.push(
                                self.render_task_card(task, idx, false, window, cx)
                                    .into_any_element(),
                            );
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
        if self.pending_deletion.is_some() {
            self.focus_handle.focus(window, cx);
        }

        let (pending_count, completed_count): (usize, usize) =
            self.tasks.iter().fold((0, 0), |(p, c), task| {
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
            .track_focus(&self.focus_handle(cx))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.pending_deletion.is_none() {
                    return;
                }

                match event.keystroke.key.as_str() {
                    "enter" => {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.confirm_delete_task(cx);
                    }
                    "escape" => {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.cancel_delete_confirmation(cx);
                    }
                    _ => {}
                }
            }))
            .on_action(
                cx.listener(|this, _action: &SetReminderAction, window, cx| {
                    if let Some(task_id) = this.context_menu_task_id {
                        if let Some(task) = this.tasks.iter().find(|t| t.id == task_id).cloned() {
                            this.start_reminder(&task, window, cx);
                        }
                    }
                }),
            )
            .on_action(
                cx.listener(|this, _action: &SetReminderTodayAction, _window, cx| {
                    if let Some(task_id) = this.context_menu_task_id {
                        let now = Local::now();
                        let target = if now.hour() >= 17 {
                            now + Duration::hours(1)
                        } else {
                            now.date_naive()
                                .and_hms_opt(18, 0, 0)
                                .unwrap()
                                .and_local_timezone(Local)
                                .unwrap()
                        };
                        this.update_task_reminder(task_id, target.with_timezone(&chrono::Utc), cx);
                    }
                }),
            )
            .on_action(
                cx.listener(|this, _action: &SetReminderTomorrowAction, _window, cx| {
                    if let Some(task_id) = this.context_menu_task_id {
                        let tomorrow = (Local::now() + Duration::days(1)).date_naive();
                        let target = tomorrow
                            .and_hms_opt(9, 0, 0)
                            .unwrap()
                            .and_local_timezone(Local)
                            .unwrap();
                        this.update_task_reminder(task_id, target.with_timezone(&chrono::Utc), cx);
                    }
                }),
            )
            .on_action(
                cx.listener(|this, _action: &SetReminderNextWeekAction, _window, cx| {
                    if let Some(task_id) = this.context_menu_task_id {
                        let next_week = (Local::now() + Duration::days(7)).date_naive();
                        let target = next_week
                            .and_hms_opt(9, 0, 0)
                            .unwrap()
                            .and_local_timezone(Local)
                            .unwrap();
                        this.update_task_reminder(task_id, target.with_timezone(&chrono::Utc), cx);
                    }
                }),
            )
            .on_action(cx.listener(|this, _action: &EditTaskAction, window, cx| {
                if let Some(task_id) = this.context_menu_task_id {
                    if let Some(task) = this.tasks.iter().find(|t| t.id == task_id).cloned() {
                        this.start_edit(task, window, cx);
                    }
                }
            }))
            .on_action(
                cx.listener(|this, _action: &DeleteTaskAction, _window, cx| {
                    if let Some(task_id) = this.context_menu_task_id {
                        this.request_delete_task(task_id, cx);
                    }
                }),
            )
            .child(
                div()
                    .id("task-panel-main")
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .p(px(16.0))
                    .on_click(cx.listener(|this, _event: &ClickEvent, window, cx| {
                        if this
                            .task_detail_sidebar
                            .read(cx)
                            .current_task_id()
                            .is_some()
                        {
                            this.task_detail_sidebar.update(cx, |sidebar, cx| {
                                sidebar.close(window, cx);
                            });
                            this.handle_sidebar_close(cx);
                        }
                    }))
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!(
                                "任务 ({} 待办 / {} 已完成)",
                                pending_count, completed_count
                            )),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .on_key_down(cx.listener(
                                        |this, event: &KeyDownEvent, window, cx| {
                                            if event.keystroke.key == "enter" {
                                                this.create_task(window, cx);
                                            }
                                        },
                                    ))
                                    .child(Input::new(&self.input_state).flex_1()),
                            )
                            .child(Button::new("add-btn").child("添加").on_click(cx.listener(
                                |this, _event: &ClickEvent, window, cx| {
                                    this.create_task(window, cx);
                                },
                            ))),
                    )
                    .child(self.render_toolbar(cx))
                    .child(self.render_tag_filter(cx))
                    .when(self.reminder_task_id.is_some(), |el| {
                        if let (Some(ref dp), Some(ref ti)) =
                            (&self.reminder_date_picker, &self.reminder_time_input)
                        {
                            let dp_clone = dp.clone();
                            let ti_clone = ti.clone();
                            let task_name = self
                                .reminder_task_id
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
                                                    .child("⏰ 设置提醒时间"),
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(rgb(0x666666))
                                                    .child(task_name),
                                            ),
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
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(rgb(0x888888))
                                                            .child("日期"),
                                                    )
                                                    .child(
                                                        DatePicker::new(&dp_clone)
                                                            .cleanable(true)
                                                            .number_of_months(1),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .w(px(100.0))
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(4.0))
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(rgb(0x888888))
                                                            .child("时间 (HH:MM)"),
                                                    )
                                                    .child(Input::new(&ti_clone)),
                                            ),
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
                                                    .on_click(cx.listener(
                                                        |this, _event, window, cx| {
                                                            this.cancel_reminder(window, cx);
                                                        },
                                                    )),
                                            )
                                            .child(
                                                Button::new("save-reminder")
                                                    .child("设定")
                                                    .on_click(cx.listener(
                                                        |this, _event, window, cx| {
                                                            this.save_reminder(window, cx);
                                                        },
                                                    )),
                                            ),
                                    ),
                            )
                        } else {
                            el
                        }
                    })
                    .child(
                        div()
                            .id("task-view-container")
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .child(match self.current_view {
                                TaskView::List => {
                                    self.render_list_view(window, cx).into_any_element()
                                }
                                TaskView::Matrix => self.render_matrix_view(window, cx),
                            }),
                    )
                    .children(self.render_custom_context_menu(cx)),
            )
            .child(self.task_detail_sidebar.clone())
            .children(self.render_delete_confirmation(cx))
    }
}

impl Focusable for TaskPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
