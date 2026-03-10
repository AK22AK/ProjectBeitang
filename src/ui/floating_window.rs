use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState, Escape};
use crate::models::{Priority, Record};
use crate::store::Store;

pub struct QuickAddWindow {
    store: Store,
    input_state: Entity<InputState>,
    _subscription: Subscription,
    is_note_mode: bool,
    focus_handle: FocusHandle,
    pub hide_app_on_close: bool,
}

impl QuickAddWindow {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入任务内容 (Enter 保存, Esc 取消)")
        });

        let _subscription = cx.subscribe_in(
            &input_state,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => {
                        this.submit(cx);
                        let hide = this.hide_app_on_close;
                        window.remove_window();
                        if hide {
                            cx.hide();
                        }
                    }
                    _ => {}
                }
            },
        );

        Self {
            store,
            input_state,
            _subscription,
            is_note_mode: false,
            focus_handle,
            hide_app_on_close: false,
        }
    }

    pub fn new_for_note(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入笔记内容，第一行自动作为标题 (Enter 保存, Esc 取消)")
        });

        let _subscription = cx.subscribe_in(
            &input_state,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => {
                        this.submit(cx);
                        let hide = this.hide_app_on_close;
                        window.remove_window();
                        if hide {
                            cx.hide();
                        }
                    }
                    _ => {}
                }
            },
        );

        Self {
            store,
            input_state,
            _subscription,
            is_note_mode: true,
            focus_handle,
            hide_app_on_close: false,
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.is_note_mode {
            self.submit_note(cx);
        } else {
            self.submit_task(cx);
        }
    }

    fn submit_task(&mut self, cx: &mut Context<Self>) {
        let text = self.input_state.read(cx).text().to_string();
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

        cx.emit(DismissEvent);
    }

    fn submit_note(&mut self, cx: &mut Context<Self>) {
        let text = self.input_state.read(cx).text().to_string();
        if text.trim().is_empty() {
            return;
        }

        // 直接使用输入内容，第一行作为标题是显示时的逻辑
        let note = Record::new_note(text);

        let store = self.store.clone();
        cx.spawn(async move |_view, _cx| {
            if let Err(e) = store.create_record(note).await {
                eprintln!("[QuickAdd] Failed to create note: {}", e);
            }
        }).detach();

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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 将焦点直接赋予给文本输入框
        self.input_state.update(cx, |input, cx| input.focus(window, cx));

        div()
            .size_full()
            .p(px(16.0))
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(|this, _action: &Escape, window, cx| {
                let hide = this.hide_app_on_close;
                window.remove_window();
                if hide {
                    cx.hide();
                }
            }))
            .child(Input::new(&self.input_state))
    }
}

// 简化的优先级解析
fn parse_quick_input(input: &str) -> (String, Priority) {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix("!!").or_else(|| trimmed.strip_prefix("！！")) {
        (rest.trim_start().to_string(), Priority::High)
    } else if let Some(rest) = trimmed.strip_prefix("!").or_else(|| trimmed.strip_prefix("！")) {
        (rest.trim_start().to_string(), Priority::Medium)
    } else {
        (trimmed.to_string(), Priority::Low)
    }
}
