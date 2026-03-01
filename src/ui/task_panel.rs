use crate::models::{Priority, Record};
use crate::store::Store;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::button::Button;
use uuid::Uuid;

pub struct TaskPanel {
    store: Store,
    tasks: Vec<Record>,
    input_state: Entity<InputState>,
    _subscription: Subscription,
}

impl TaskPanel {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 创建 InputState，设置 placeholder
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("!! 高优先级任务 | ! 普通任务 | 直接输入")
        });

        // 订阅输入事件
        let _subscription = cx.subscribe_in(
            &input_state,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => {
                        this.create_task(window, cx);
                    }
                    _ => {}
                }
            },
        );

        let mut panel = Self {
            store,
            tasks: Vec::new(),
            input_state,
            _subscription,
        };
        panel.load_tasks(cx);
        panel
    }

    fn load_tasks(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            match store.get_tasks(false).await {
                Ok(tasks) => {
                    view.update(cx, |panel, _cx| {
                        panel.tasks = tasks;
                    }).ok();
                }
                Err(e) => eprintln!("Failed to load tasks: {}", e),
            }
        }).detach();
    }

    fn create_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input_state.read(cx).text().to_string();
        if text.trim().is_empty() {
            return;
        }

        let (content, priority) = self.parse_input(&text);
        let task = Record::new_task(content, priority);

        // 清空输入框
        self.input_state.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });

        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            match store.create_record(task).await {
                Ok(_) => {
                    view.update(cx, |panel, cx| {
                        panel.load_tasks(cx);
                    }).ok();
                }
                Err(e) => eprintln!("Failed to create task: {}", e),
            }
        }).detach();
    }

    fn parse_input(&self, input: &str) -> (String, Priority) {
        let trimmed = input.trim();
        if trimmed.starts_with("!! ") {
            (trimmed[3..].to_string(), Priority::High)
        } else if trimmed.starts_with("! ") {
            (trimmed[2..].to_string(), Priority::Medium)
        } else {
            (trimmed.to_string(), Priority::Low)
        }
    }

    fn complete_task(&mut self, task_id: Uuid, cx: &mut Context<Self>) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.complete();
            let updated_task = task.clone();
            let store = self.store.clone();

            cx.spawn(async move |view, cx| {
                match store.update_record(updated_task).await {
                    Ok(_) => {
                        view.update(cx, |panel, cx| {
                            panel.load_tasks(cx);
                        }).ok();
                    }
                    Err(e) => eprintln!("Failed to complete task: {}", e),
                }
            }).detach();
        }
    }
}

impl Render for TaskPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("任务")
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .bg(rgb(0xffffff))
                            .rounded(px(6.0))
                            .p(px(4.0))
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
            .child(
                div()
                    .id("task-list")
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .overflow_y_scroll()
                    .children(self.tasks.clone().into_iter().enumerate().map(|(idx, task)| {
                        let task_id = task.id;
                        let is_completed = task.is_completed();
                        let priority_emoji = match task.priority {
                            Some(Priority::High) => "🔴",
                            Some(Priority::Medium) => "🟡",
                            Some(Priority::Low) => "🟢",
                            None => "⚪",
                        };

                        div()
                            .id(idx)
                            .flex()
                            .gap(px(8.0))
                            .items_center()
                            .px(px(12.0))
                            .py(px(8.0))
                            .rounded(px(6.0))
                            .bg(rgb(0x252525))
                            .child(
                                div()
                                    .cursor_pointer()
                                    .child(if is_completed { "☑" } else { "☐" })
                                    .id(("checkbox", idx))
                                    .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                                        if !is_completed {
                                            this.complete_task(task_id, cx);
                                        }
                                    }))
                            )
                            .child(priority_emoji)
                            .child(task.content)
                    }))
            )
    }
}
