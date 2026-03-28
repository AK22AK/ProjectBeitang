use gpui::*;
use gpui::prelude::FluentBuilder as _;
use gpui_component::input::{Input, InputEvent, InputState, Escape};
use gpui_component::button::Button;
use gpui_component::Selectable;
use crate::models::{Priority, Record};
use crate::store::Store;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum InputMode {
    Task,
    Record,
}

impl InputMode {
    fn label(&self) -> &'static str {
        match self {
            InputMode::Task => "任务",
            InputMode::Record => "记录",
        }
    }

    fn placeholder(&self) -> &'static str {
        match self {
            InputMode::Task => "输入任务内容 (Enter 保存, Esc 取消, Tab 切换模式)",
            InputMode::Record => "输入记录内容 (Enter 保存, Esc 取消, Tab 切换模式)",
        }
    }
}

pub struct QuickAddWindow {
    store: Store,
    input_state: Entity<InputState>,
    _subscription: Subscription,
    mode: InputMode,
    focus_handle: FocusHandle,
    pub hide_app_on_close: bool,
}

impl QuickAddWindow {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let mode = InputMode::Record; // 默认记录模式

        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(mode.placeholder())
        });

        let _subscription = cx.subscribe_in(
            &input_state,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.submit(cx);
                    let hide = this.hide_app_on_close;
                    window.remove_window();
                    if hide {
                        cx.hide();
                    }
                }
            },
        );

        Self {
            store,
            input_state,
            _subscription,
            mode,
            focus_handle,
            hide_app_on_close: false,
        }
    }

    fn set_mode(&mut self, mode: InputMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.mode = mode;
            self.input_state.update(cx, |input, cx| {
                input.set_placeholder(mode.placeholder(), window, cx);
            });
            cx.notify();
        }
    }

    fn toggle_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new_mode = match self.mode {
            InputMode::Task => InputMode::Record,
            InputMode::Record => InputMode::Task,
        };
        self.set_mode(new_mode, window, cx);
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        match self.mode {
            InputMode::Task => self.submit_task(cx),
            InputMode::Record => self.submit_record(cx),
        }
    }

    fn submit_task(&mut self, cx: &mut Context<Self>) {
        let text = self.input_state.read(cx).text().to_string();
        if text.trim().is_empty() {
            return;
        }

        let (title, priority, tags, people) = parse_task_input(&text);
        // 快速添加时，输入内容作为 title，content 初始为空
        let mut task = Record::new_task(title, String::new(), priority);
        
        // 添加标签和人物
        for tag in tags {
            task.tags.push(tag);
        }
        for person in people {
            task.persons.push(person);
        }

        let store = self.store.clone();
        cx.spawn(async move |_view, _cx| {
            if let Err(e) = store.create_record(task).await {
                eprintln!("[QuickAdd] Failed to create task: {}", e);
            }
        }).detach();

        cx.emit(DismissEvent);
    }

    fn submit_record(&mut self, cx: &mut Context<Self>) {
        let text = self.input_state.read(cx).text().to_string();
        if text.trim().is_empty() {
            return;
        }

        let (content, tags, people) = parse_record_input(&text);
        let mut record = Record::new_note(if content.is_empty() { text } else { content });
        
        // 添加标签和人物
        for tag in tags {
            record.tags.push(tag);
        }
        for person in people {
            record.persons.push(person);
        }

        let store = self.store.clone();
        cx.spawn(async move |_view, _cx| {
            if let Err(e) = store.create_record(record).await {
                eprintln!("[QuickAdd] Failed to create record: {}", e);
            }
        }).detach();

        cx.emit(DismissEvent);
    }

    fn render_mode_tab(&self, mode: InputMode, cx: &mut Context<Self>) -> impl IntoElement {
        let is_active = self.mode == mode;
        let label = mode.label();
        
        Button::new(label)
            .label(label)
            .selected(is_active)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.set_mode(mode, window, cx);
            }))
    }

    fn render_tips(&self) -> impl IntoElement {
        let task_tips = "任务模式: !!高优先级  !普通优先级  #标签  @人物";
        let record_tips = "记录模式: #标签  @人物";

        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x666666))
                    .child("提示:")
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x999999))
                    .child(task_tips)
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x999999))
                    .child(record_tips)
            )
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
            .flex()
            .flex_col()
            .gap(px(12.0))
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(|this, _action: &Escape, window, cx| {
                let hide = this.hide_app_on_close;
                window.remove_window();
                if hide {
                    cx.hide();
                }
            }))
            // Tab 键切换模式
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key.as_str() == "tab" {
                    this.toggle_mode(window, cx);
                }
            }))
            // 输入区域
            .child(
                div()
                    .flex_1()
                    .child(Input::new(&self.input_state))
            )
            // Tab 切换按钮
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(self.render_mode_tab(InputMode::Task, cx))
                    .child(self.render_mode_tab(InputMode::Record, cx))
            )
            // 提示区域
            .child(self.render_tips())
            // 查看记录按钮（仅在记录模式下显示）
            .when(self.mode == InputMode::Record, |el| {
                el.child(
                    div()
                        .flex()
                        .justify_end()
                        .child(
                            Button::new("view-records-btn")
                                .child("查看记录 (Cmd+5)")
                                .text_color(rgb(0x1890ff))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    let hide = this.hide_app_on_close;
                                    window.remove_window();
                                    if hide {
                                        cx.hide();
                                    }
                                }))
                        )
                )
            })
    }
}

/// 解析任务输入，返回 (内容, 优先级, 标签列表, 人物列表)
fn parse_task_input(input: &str) -> (String, Priority, Vec<String>, Vec<String>) {
    let (content_without_tags, tags, people) = parse_tags_and_people(input);
    let (content, priority) = parse_priority(&content_without_tags);
    (content, priority, tags, people)
}

/// 解析记录输入，返回 (内容, 标签列表, 人物列表)
fn parse_record_input(input: &str) -> (String, Vec<String>, Vec<String>) {
    parse_tags_and_people(input)
}

/// 解析优先级，返回 (去除优先级的内容, 优先级)
fn parse_priority(input: &str) -> (String, Priority) {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix("!!").or_else(|| trimmed.strip_prefix("！！")) {
        (rest.trim_start().to_string(), Priority::High)
    } else if let Some(rest) = trimmed.strip_prefix("!").or_else(|| trimmed.strip_prefix("！")) {
        (rest.trim_start().to_string(), Priority::Medium)
    } else {
        (trimmed.to_string(), Priority::Low)
    }
}

/// 解析标签和人物，返回 (纯内容, 标签列表, 人物列表)
fn parse_tags_and_people(input: &str) -> (String, Vec<String>, Vec<String>) {
    let mut tags = Vec::new();
    let mut people = Vec::new();
    let mut content_parts = Vec::new();

    // 按空白字符分割输入
    for word in input.split_whitespace() {
        if let Some(tag) = word.strip_prefix('#') {
            if !tag.is_empty() {
                tags.push(tag.to_string());
            }
        } else if let Some(person) = word.strip_prefix('@') {
            if !person.is_empty() {
                people.push(person.to_string());
            }
        } else {
            content_parts.push(word);
        }
    }

    let content = content_parts.join(" ");
    (content, tags, people)
}
