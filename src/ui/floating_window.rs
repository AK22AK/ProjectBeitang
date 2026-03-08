use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use crate::models::{Priority, Record};
use crate::store::Store;

pub struct QuickAddWindow {
    store: Store,
    title_input: Entity<InputState>,
    content_input: Option<Entity<InputState>>,
    _title_subscription: Subscription,
    _content_subscription: Option<Subscription>,
    is_note_mode: bool,
    focus_handle: FocusHandle,
}

impl QuickAddWindow {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入任务内容 (Enter 保存, Esc 取消)")
        });

        let _title_subscription = cx.subscribe_in(
            &title_input,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => {
                        this.submit(window, cx);
                    }
                    _ => {}
                }
            },
        );

        Self {
            store,
            title_input,
            content_input: None,
            _title_subscription,
            _content_subscription: None,
            is_note_mode: false,
            focus_handle,
        }
    }

    pub fn new_for_note(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("笔记标题")
        });

        let content_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("笔记内容 (Enter 保存, Esc 取消)")
        });

        let _title_subscription = cx.subscribe_in(
            &title_input,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => {
                        // Move focus to content input
                        if let Some(ref content) = this.content_input {
                            content.focus_handle(cx).focus(window, cx);
                        }
                    }
                    _ => {}
                }
            },
        );

        let _content_subscription = cx.subscribe_in(
            &content_input,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => {
                        this.submit(window, cx);
                    }
                    _ => {}
                }
            },
        );

        Self {
            store,
            title_input,
            content_input: Some(content_input),
            _title_subscription,
            _content_subscription: Some(_content_subscription),
            is_note_mode: true,
            focus_handle,
        }
    }

    fn submit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_note_mode {
            self.submit_note(cx);
        } else {
            self.submit_task(cx);
        }
    }

    fn submit_task(&mut self, cx: &mut Context<Self>) {
        let text = self.title_input.read(cx).text().to_string();
        if text.trim().is_empty() {
            return;
        }

        let (content, priority) = parse_quick_input(&text);
        let task = Record::new_task(content, priority);

        let store = self.store.clone();
        cx.spawn(async move |_view, _cx| {
            if let Err(e) = store.create_record(task).await {
                eprintln!("[QuickAdd] Failed to create task: {}", e);
            }
        }).detach();

        // 关闭窗口
        cx.emit(DismissEvent);
    }

    fn submit_note(&mut self, cx: &mut Context<Self>) {
        let title = self.title_input.read(cx).text().to_string();
        let content = self.content_input.as_ref()
            .map(|input| input.read(cx).text().to_string())
            .unwrap_or_default();

        if title.trim().is_empty() && content.trim().is_empty() {
            return;
        }

        let note_title = if title.trim().is_empty() {
            "无标题笔记".to_string()
        } else {
            title.trim().to_string()
        };

        // Combine title and content for storage
        let full_content = format!("{}\n\n{}", note_title, content);
        let note = Record::new_note(full_content);

        let store = self.store.clone();
        cx.spawn(async move |_view, _cx| {
            if let Err(e) = store.create_record(note).await {
                eprintln!("[QuickAdd] Failed to create note: {}", e);
            }
        }).detach();

        // 关闭窗口
        cx.emit(DismissEvent);
    }
}

impl EventEmitter<DismissEvent> for QuickAddWindow {}

impl Focusable for QuickAddWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for QuickAddWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Request focus when window is rendered
        self.focus_handle(cx).focus(_window, cx);

        if self.is_note_mode {
            div()
                .size_full()
                .p(px(16.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .track_focus(&self.focus_handle(cx))
                .on_key_down(cx.listener(|_this, event: &KeyDownEvent, _window, cx| {
                    if event.keystroke.key == "escape" {
                        cx.emit(DismissEvent);
                    }
                }))
                .child(Input::new(&self.title_input))
                .children(self.content_input.as_ref().map(|input| {
                    Input::new(input).into_any_element()
                }))
        } else {
            div()
                .size_full()
                .p(px(16.0))
                .track_focus(&self.focus_handle(cx))
                .on_key_down(cx.listener(|_this, event: &KeyDownEvent, _window, cx| {
                    if event.keystroke.key == "escape" {
                        cx.emit(DismissEvent);
                    }
                }))
                .child(Input::new(&self.title_input))
        }
    }
}

// 简化的优先级解析
fn parse_quick_input(input: &str) -> (String, Priority) {
    let trimmed = input.trim();
    if trimmed.starts_with("!!") {
        (trimmed[2..].trim_start().to_string(), Priority::High)
    } else if trimmed.starts_with("!") {
        (trimmed[1..].trim_start().to_string(), Priority::Medium)
    } else {
        (trimmed.to_string(), Priority::Low)
    }
}
