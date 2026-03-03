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
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("!! 高优先级 | ! 普通优先级 | 直接输入")
        });

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
        if trimmed.starts_with("!!") {
            // 高优先级：!! 或 !!空格
            let content = trimmed[2..].trim_start();
            (content.to_string(), Priority::High)
        } else if trimmed.starts_with("!") {
            // 普通优先级：! 或 !空格
            let content = trimmed[1..].trim_start();
            (content.to_string(), Priority::Medium)
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
        eprintln!("[TaskPanel] Rendering {} tasks", self.tasks.len());
        for (i, task) in self.tasks.iter().enumerate() {
            eprintln!("[TaskPanel] Task {}: content='{}', priority={:?}", i, task.content, task.priority);
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("任务 ({})", self.tasks.len()))
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        Input::new(&self.input_state)
                            .flex_1()
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
                            .bg(rgb(0xe8e8e8))
                            .text_color(rgb(0x000000))  // 明确设置黑色文字
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
                            .child(
                                div()
                                    .text_color(rgb(0x000000))  // 确保内容文字是黑色
                                    .child(task.content.clone())
                            )
                    }))
            )
    }
}
