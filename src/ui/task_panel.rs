use crate::models::{Priority, Record};
use crate::store::Store;
use gpui::*;
use gpui::prelude::*;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::button::Button;
use gpui_component::InteractiveElementExt;
use gpui_component::scroll::ScrollableElement;
use uuid::Uuid;

pub struct TaskPanel {
    store: Store,
    tasks: Vec<Record>,
    input_state: Entity<InputState>,
    _subscription: Subscription,
    // 编辑状态
    editing_task_id: Option<uuid::Uuid>,
    edit_input_state: Option<Entity<InputState>>,
    _edit_subscription: Option<Subscription>,
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
            editing_task_id: None,
            edit_input_state: None,
            _edit_subscription: None,
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
        });

        let _edit_subscription = cx.subscribe_in(
            &edit_input,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => {
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
}

impl Render for TaskPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 分离已完成和未完成任务
        let (pending_tasks, completed_tasks): (Vec<_>, Vec<_>) = self.tasks.iter()
            .cloned()
            .partition(|task| task.completed_at.is_none());

        let pending_count = pending_tasks.len();
        let completed_count = completed_tasks.len();

        eprintln!("[TaskPanel] Rendering {} pending, {} completed", pending_count, completed_count);

        // 渲染单个任务项的辅助函数
        let render_task = |task: Record, idx: usize, cx: &mut Context<Self>| {
            let task_id = task.id;
            let is_completed = task.completed_at.is_some();
            let is_editing = self.editing_task_id == Some(task_id);
            let priority_emoji = match task.priority {
                Some(Priority::High) => "🔴",
                Some(Priority::Medium) => "🟡",
                Some(Priority::Low) => "🟢",
                None => "⚪",
            };

            if is_editing {
                if let Some(ref edit_input) = self.edit_input_state {
                    let edit_input_clone = edit_input.clone();
                    return div()
                        .id(("edit", idx))
                        .flex()
                        .gap(px(8.0))
                        .items_center()
                        .px(px(12.0))
                        .py(px(8.0))
                        .rounded(px(6.0))
                        .bg(rgb(0xd0e8ff))
                        .child(Input::new(&edit_input_clone).flex_1())
                        .child(Button::new("save-btn").child("保存").on_click(cx.listener(|this, _event, window, cx| {
                            this.save_edit(window, cx);
                        })))
                        .child(Button::new("cancel-btn").child("取消").on_click(cx.listener(|this, _event, window, cx| {
                            this.cancel_edit(window, cx);
                        })))
                        .into_any_element();
                }
            }

            div()
                .id(("task", idx))
                .flex()
                .gap(px(8.0))
                .items_center()
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(6.0))
                .bg(if is_completed { rgb(0xc8c8c8) } else { rgb(0xe8e8e8) })
                .text_color(rgb(0x000000))
                .cursor_pointer()
                .on_double_click(cx.listener({
                    let task = task.clone();
                    move |this, _event: &ClickEvent, window, cx| {
                        this.start_edit(task.clone(), window, cx);
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
                .child(priority_emoji)
                .child(
                    div()
                        .flex_1()
                        .text_color(if is_completed { rgb(0x888888) } else { rgb(0x000000) })
                        .child(if is_completed {
                            format!("[已完成] {}", task.content)
                        } else {
                            task.content.clone()
                        })
                )
                .into_any_element()
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
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
            // 待办任务区域
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
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
                                    .mt(px(16.0))
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0x999999))
                                    .child(format!("已完成 ({})", completed_count))
                                    .into_any_element()
                            );
                            for (idx, task) in completed_tasks.iter().cloned().enumerate() {
                                elements.push(render_task(task, idx, cx));
                            }
                        }

                        elements
                    })
            )
    }
}
