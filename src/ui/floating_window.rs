use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use crate::models::{Priority, Record};
use crate::store::Store;

pub struct QuickAddWindow {
    store: Store,
    input_state: Entity<InputState>,
    _subscription: Subscription,
}

impl QuickAddWindow {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
                        this.submit(window, cx);
                    }
                    _ => {}
                }
            },
        );

        Self {
            store,
            input_state,
            _subscription,
        }
    }

    fn submit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
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

        // 关闭窗口
        cx.emit(DismissEvent);
    }
}

impl EventEmitter<DismissEvent> for QuickAddWindow {}

impl Render for QuickAddWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p(px(16.0))
            .child(Input::new(&self.input_state))
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
