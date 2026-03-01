use crate::models::{Priority, Record};
use crate::store::Store;
use gpui::*;
use uuid::Uuid;

pub struct TaskPanel {
    store: Store,
    tasks: Vec<Record>,
    input_value: String,
    focus_handle: FocusHandle,
}

impl TaskPanel {
    pub fn new(store: Store, cx: &mut Context<Self>) -> Self {
        let mut panel = Self {
            store,
            tasks: Vec::new(),
            input_value: String::new(),
            focus_handle: cx.focus_handle(),
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

    fn create_task(&mut self, cx: &mut Context<Self>) {
        if self.input_value.trim().is_empty() {
            return;
        }

        let (content, priority) = self.parse_input(&self.input_value);
        let task = Record::new_task(content, priority);

        // Clear input
        self.input_value.clear();

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
        let input_value = self.input_value.clone();

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
                            .px(px(12.0))
                            .py(px(8.0))
                            .rounded(px(6.0))
                            .bg(rgb(0x252525))
                            .child(input_value.clone())
                            .id("input-display")
                            .focusable()
                            .track_focus(&self.focus_handle)
                            .on_click(cx.listener(|this, _event: &ClickEvent, window, cx| {
                                this.focus_handle.focus(window, cx);
                            }))
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                                let key = &event.keystroke.key;
                                let has_modifiers = event.keystroke.modifiers.control
                                    || event.keystroke.modifiers.alt
                                    || event.keystroke.modifiers.platform
                                    || event.keystroke.modifiers.function;

                                let mut changed = false;
                                if key.len() == 1 && !has_modifiers && !event.keystroke.modifiers.shift {
                                    this.input_value.push_str(key);
                                    changed = true;
                                } else if key.len() == 1 && event.keystroke.modifiers.shift {
                                    this.input_value.push_str(&key.to_uppercase());
                                    changed = true;
                                } else if key == "backspace" {
                                    this.input_value.pop();
                                    changed = true;
                                } else if key == "enter" {
                                    this.create_task(cx);
                                    return;
                                } else if key == "space" {
                                    this.input_value.push(' ');
                                    changed = true;
                                }

                                if changed {
                                    cx.notify();
                                }
                            }))
                    )
                    .child(
                        div()
                            .px(px(16.0))
                            .py(px(8.0))
                            .rounded(px(6.0))
                            .bg(rgb(0x3a3a3a))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x4a4a4a)))
                            .child("添加")
                            .id("add-button")
                            .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                this.create_task(cx);
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
