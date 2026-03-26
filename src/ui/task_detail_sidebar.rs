use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use gpui::{prelude::*, Entity, Window};
use gpui_component::{
    button::{Button, ButtonVariants},
    date_picker::{DatePicker, DatePickerState},
    h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement,
    v_flex,
};

use crate::models::{Priority, Record, TaskStatus};

pub struct TaskDetailSidebar {
    current_task_id: Option<String>,
    task_content: String,
    priority: Option<Priority>,
    status: Option<TaskStatus>,
    due_date: Option<DateTime<Utc>>,
    cancel_reason: Option<String>,
    date_picker: Option<Entity<DatePickerState>>,
    time_input: Option<Entity<InputState>>,
    on_save: Option<Box<dyn Fn(SavePayload, &mut Context<Self>) + Send + Sync>>,
    on_close: Option<Box<dyn Fn(&mut Context<Self>) + Send + Sync>>,
}

/// 保存时的数据载荷
#[derive(Debug, Clone)]
pub struct SavePayload {
    pub task_id: String,
    pub priority: Priority,
    pub status: TaskStatus,
    pub due_date: Option<DateTime<Utc>>,
    pub cancel_reason: Option<String>,
}

/// 侧边栏显示状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SidebarState {
    Hidden,
    Visible,
}

impl TaskDetailSidebar {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            current_task_id: None,
            task_content: String::new(),
            priority: None,
            status: None,
            due_date: None,
            cancel_reason: None,
            date_picker: None,
            time_input: None,
            on_save: None,
            on_close: None,
        }
    }

    pub fn on_save<F>(&mut self, callback: F)
    where
        F: Fn(SavePayload, &mut Context<Self>) + Send + Sync + 'static,
    {
        self.on_save = Some(Box::new(callback));
    }

    pub fn on_close<F>(&mut self, callback: F)
    where
        F: Fn(&mut Context<Self>) + Send + Sync + 'static,
    {
        self.on_close = Some(Box::new(callback));
    }

    /// 显示任务详情 - 只在 task_id 变化时才重建 UI 状态
    pub fn show_task(&mut self, task: &Record, window: &mut Window, cx: &mut Context<Self>) {
        let task_id = task.id.to_string();

        // 关键：如果已经在显示同一个任务，什么都不做
        if self.current_task_id.as_ref() == Some(&task_id) {
            return;
        }

        // 更新任务数据
        self.current_task_id = Some(task_id);
        self.task_content = task.content.clone();
        self.priority = task.priority.clone();
        self.status = task.status.clone();
        self.due_date = task.due_date;
        self.cancel_reason = task.cancelled_reason.clone();

        // 初始化或更新日期选择器状态
        let (init_date, init_time_str) = if let Some(due) = self.due_date {
            let local = due.with_timezone(&Local);
            (
                local.naive_local().date(),
                local.format("%H:%M").to_string(),
            )
        } else {
            let now = Local::now();
            (now.naive_local().date(), now.format("%H:%M").to_string())
        };

        // 如果日期选择器已存在，只更新值；否则创建新的
        if let (Some(ref dp), Some(ref ti)) = (&self.date_picker, &self.time_input) {
            dp.update(cx, |state, cx| {
                state.set_date(init_date, window, cx);
            });
            ti.update(cx, |state, cx| {
                state.set_value(&init_time_str, window, cx);
            });
        } else {
            // 创建新的状态
            self.init_picker_states(init_date, &init_time_str, window, cx);
        }

        cx.notify();
    }

    /// 关闭侧边栏
    pub fn close(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.current_task_id = None;
        cx.notify();
    }

    /// 获取当前状态
    pub fn state(&self) -> SidebarState {
        if self.current_task_id.is_some() {
            SidebarState::Visible
        } else {
            SidebarState::Hidden
        }
    }

    /// 获取当前任务 ID
    pub fn current_task_id(&self) -> Option<&str> {
        self.current_task_id.as_deref()
    }

    fn init_picker_states(
        &mut self,
        init_date: NaiveDate,
        init_time_str: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let date_picker = cx.new(|cx| {
            let mut picker = DatePickerState::new(window, cx).date_format("%Y-%m-%d");
            picker.set_date(init_date, window, cx);
            picker
        });

        let time_str = init_time_str.to_string();
        let time_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx);
            input.set_value(&time_str, window, cx);
            input
        });

        self.date_picker = Some(date_picker);
        self.time_input = Some(time_input);
    }

    fn save_changes(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let (Some(ref task_id), Some(ref priority), Some(ref status)) =
            (&self.current_task_id, &self.priority, &self.status)
        {
            // 从选择器读取当前值
            let due_date = self.date_picker.as_ref().and_then(|dp| {
                let date_range = dp.read(cx).date();
                let start_date = date_range.start();
                start_date.and_then(|d| {
                    let time_str = self
                        .time_input
                        .as_ref()
                        .map(|ti| ti.read(cx).value().to_string())
                        .unwrap_or_else(|| "00:00".to_string());

                    self.parse_due_date(d, &time_str)
                })
            });

            let payload = SavePayload {
                task_id: task_id.clone(),
                priority: priority.clone(),
                status: status.clone(),
                due_date,
                cancel_reason: self.cancel_reason.clone(),
            };

            if let Some(ref callback) = self.on_save {
                callback(payload, cx);
            }
        }
    }

    fn parse_due_date(&self, date: NaiveDate, time_str: &str) -> Option<DateTime<Utc>> {
        let time_parts: Vec<&str> = time_str.split(':').collect();
        let hour = time_parts
            .first()
            .and_then(|h| h.parse().ok())
            .unwrap_or(0u32);
        let minute = time_parts
            .get(1)
            .and_then(|m| m.parse().ok())
            .unwrap_or(0u32);

        date.and_hms_opt(hour, minute, 0)
            .and_then(|dt| Local.from_local_datetime(&dt).single())
            .map(|dt| dt.with_timezone(&Utc))
    }

    fn set_status(&mut self, status: TaskStatus, _window: &mut Window, cx: &mut Context<Self>) {
        self.status = Some(status);
        cx.notify();
    }

    fn set_priority(&mut self, priority: Priority, _window: &mut Window, cx: &mut Context<Self>) {
        self.priority = Some(priority);
        cx.notify();
    }

    fn render_status_button(&self, status: TaskStatus, cx: &mut Context<Self>) -> impl IntoElement {
        let (label, color) = match status {
            TaskStatus::Todo => ("待办", gpui::rgb(0x999999)),
            TaskStatus::InProgress => ("进行中", gpui::rgb(0x1890ff)),
            TaskStatus::Done => ("已完成", gpui::rgb(0x52c41a)),
            TaskStatus::Cancelled => ("已取消", gpui::rgb(0xff4d4f)),
        };

        let is_selected = self.status == Some(status.clone());

        Button::new(format!("sidebar-status-{:?}", status))
            .child(label)
            .when(is_selected, |b| {
                b.with_variant(gpui_component::button::ButtonVariant::Primary)
            })
            .when(!is_selected, |b| b.text_color(color))
            .on_click(
                cx.listener(move |this, _event: &gpui::ClickEvent, window, cx| {
                    this.set_status(status.clone(), window, cx);
                    cx.stop_propagation();
                }),
            )
    }

    fn render_priority_button(
        &self,
        priority: Priority,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (label, color) = match priority {
            Priority::High => ("高", gpui::rgb(0xff4d4f)),
            Priority::Medium => ("中", gpui::rgb(0xfaad14)),
            Priority::Low => ("低", gpui::rgb(0x52c41a)),
        };

        let is_selected = self.priority == Some(priority.clone());

        Button::new(format!("sidebar-priority-{:?}", priority))
            .child(label)
            .when(is_selected, |b| {
                b.with_variant(gpui_component::button::ButtonVariant::Primary)
            })
            .when(!is_selected, |b| b.text_color(color))
            .on_click(
                cx.listener(move |this, _event: &gpui::ClickEvent, window, cx| {
                    this.set_priority(priority.clone(), window, cx);
                    cx.stop_propagation();
                }),
            )
    }
}

impl Render for TaskDetailSidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_visible = self.current_task_id.is_some();
        let task_content = self.task_content.clone();
        let dp_clone = self.date_picker.clone();
        let ti_clone = self.time_input.clone();

        gpui::div()
            .absolute()
            .top(gpui::px(0.0))
            .right(gpui::px(0.0))
            .bottom(gpui::px(0.0))
            .w(gpui::px(360.0))
            .when(!is_visible, |el| el.invisible())
            .overflow_hidden()
            .border_l_1()
            .border_color(gpui::rgb(0xe8e8e8))
            .bg(gpui::rgb(0xffffff))
            .cursor_default()
            .child(
                gpui::div()
                    .w(gpui::px(360.0))
                    .h_full()
                    .flex()
                    .flex_col()
                    .border_l_1()
                    .border_color(gpui::rgb(0xe8e8e8))
                    .bg(gpui::rgb(0xffffff))
                    .cursor_default()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|_this, _event, _window, cx| {
                            cx.stop_propagation();
                        }),
                    )
                    .child(
                        gpui::div()
                            .p(gpui::px(12.0))
                            .border_b_1()
                            .border_color(gpui::rgb(0xe8e8e8))
                            .cursor_default()
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        gpui::div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child("任务详情"),
                                    )
                                    .child(
                                        Button::new("sidebar-close-detail").child("✕").on_click(
                                            cx.listener(|this, _event, window, cx| {
                                                this.close(window, cx);
                                                if let Some(ref callback) = this.on_close {
                                                    callback(cx);
                                                }
                                            }),
                                        ),
                                    ),
                            ),
                    )
                    .child(
                        gpui::div()
                            .flex_1()
                            .p(gpui::px(12.0))
                            .overflow_y_scrollbar()
                            .cursor_default()
                            .child(
                                v_flex()
                                    .gap(gpui::px(12.0))
                                    .child(
                                        v_flex()
                                            .gap(gpui::px(6.0))
                                            .child(
                                                gpui::div()
                                                    .text_xs()
                                                    .text_color(gpui::rgb(0x666666))
                                                    .child("内容"),
                                            )
                                            .child(gpui::div().text_sm().child(task_content)),
                                    )
                                    .child(
                                        v_flex()
                                            .gap(gpui::px(6.0))
                                            .child(
                                                gpui::div()
                                                    .text_xs()
                                                    .text_color(gpui::rgb(0x666666))
                                                    .child("状态"),
                                            )
                                            .child(
                                                h_flex()
                                                    .gap(gpui::px(6.0))
                                                    .child(
                                                        self.render_status_button(
                                                            TaskStatus::Todo,
                                                            cx,
                                                        ),
                                                    )
                                                    .child(self.render_status_button(
                                                        TaskStatus::InProgress,
                                                        cx,
                                                    ))
                                                    .child(
                                                        self.render_status_button(
                                                            TaskStatus::Done,
                                                            cx,
                                                        ),
                                                    )
                                                    .child(self.render_status_button(
                                                        TaskStatus::Cancelled,
                                                        cx,
                                                    )),
                                            ),
                                    )
                                    .child(
                                        v_flex()
                                            .gap(gpui::px(6.0))
                                            .child(
                                                gpui::div()
                                                    .text_xs()
                                                    .text_color(gpui::rgb(0x666666))
                                                    .child("优先级"),
                                            )
                                            .child(
                                                h_flex()
                                                    .gap(gpui::px(6.0))
                                                    .child(
                                                        self.render_priority_button(
                                                            Priority::High,
                                                            cx,
                                                        ),
                                                    )
                                                    .child(self.render_priority_button(
                                                        Priority::Medium,
                                                        cx,
                                                    ))
                                                    .child(
                                                        self.render_priority_button(
                                                            Priority::Low,
                                                            cx,
                                                        ),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        v_flex()
                                            .gap(gpui::px(6.0))
                                            .child(
                                                gpui::div()
                                                    .text_xs()
                                                    .text_color(gpui::rgb(0x666666))
                                                    .child("截止日期"),
                                            )
                                            .child(
                                                h_flex()
                                                    .gap(gpui::px(6.0))
                                                    .items_end()
                                                    .when_some(dp_clone.clone(), |el, dp| {
                                                        el.child(
                                                            gpui::div().flex_1().child(
                                                                DatePicker::new(&dp)
                                                                    .cleanable(true)
                                                                    .number_of_months(1),
                                                            ),
                                                        )
                                                    })
                                                    .when_some(ti_clone.clone(), |el, ti| {
                                                        el.child(
                                                            gpui::div()
                                                                .w(gpui::px(80.0))
                                                                .child(Input::new(&ti)),
                                                        )
                                                    }),
                                            ),
                                    )
                                    .when(self.status == Some(TaskStatus::Cancelled), |el| {
                                        el.child(
                                            v_flex()
                                                .gap(gpui::px(6.0))
                                                .child(
                                                    gpui::div()
                                                        .text_xs()
                                                        .text_color(gpui::rgb(0x666666))
                                                        .child("取消原因"),
                                                )
                                                .child(
                                                    gpui::div()
                                                        .text_xs()
                                                        .text_color(gpui::rgb(0x999999))
                                                        .child("（此处可添加原因输入框）"),
                                                ),
                                        )
                                    }),
                            ),
                    )
                    .child(
                        gpui::div()
                            .p(gpui::px(12.0))
                            .border_t_1()
                            .border_color(gpui::rgb(0xe8e8e8))
                            .cursor_default()
                            .child(
                                Button::new("sidebar-save-detail")
                                    .w_full()
                                    .child("保存修改")
                                    .on_click(cx.listener(|this, _event, window, cx| {
                                        this.save_changes(window, cx);
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
}
