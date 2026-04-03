use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState, Escape, IndentInline};
use gpui_component::IconName;
use crate::models::Record;
use crate::store::Store;
use crate::ui::parsing;
use std::time::Duration as StdDuration;

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

    fn submit_label(&self) -> &'static str {
        match self {
            InputMode::Task => "创建一条任务",
            InputMode::Record => "创建一条记录",
        }
    }

    fn placeholder(&self) -> &'static str {
        match self {
            InputMode::Task => "输入任务 (Enter保存, Esc取消, Tab切换)",
            InputMode::Record => "输入记录 (Enter保存, Esc取消, Tab切换)",
        }
    }
}

pub fn quick_add_window_size() -> Size<Pixels> {
    size(px(520.0), px(168.0))
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum QuickAddFeedback {
    Idle,
    EmptySubmitWarning,
    EscConfirmPending { generation: u64 },
}

pub struct QuickAddWindow {
    store: Store,
    input_state: Entity<InputState>,
    _subscription: Subscription,
    mode: InputMode,
    focus_handle: FocusHandle,
    feedback: QuickAddFeedback,
    esc_confirm_generation: u64,
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
                match event {
                    InputEvent::PressEnter { .. } => {
                        this.try_submit(window, cx);
                    }
                    InputEvent::Change => {
                        this.clear_transient_feedback(cx);
                    }
                    InputEvent::Focus | InputEvent::Blur => {}
                }
            },
        );

        Self {
            store,
            input_state,
            _subscription,
            mode,
            focus_handle,
            feedback: QuickAddFeedback::Idle,
            esc_confirm_generation: 0,
            hide_app_on_close: false,
        }
    }

    fn set_mode(&mut self, mode: InputMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.clear_transient_feedback(cx);
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

    fn input_text(&self, cx: &Context<Self>) -> String {
        self.input_state.read(cx).text().to_string()
    }

    fn has_input(&self, cx: &Context<Self>) -> bool {
        !self.input_text(cx).trim().is_empty()
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        match self.mode {
            InputMode::Task => self.submit_task(cx),
            InputMode::Record => self.submit_record(cx),
        }
    }

    fn close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let hide = self.hide_app_on_close;
        window.remove_window();
        if hide {
            cx.hide();
        }
    }

    fn submit_and_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_transient_feedback(cx);
        self.submit(cx);
        self.close_window(window, cx);
    }

    fn try_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_input(cx) {
            self.feedback = QuickAddFeedback::EmptySubmitWarning;
            cx.notify();
            return;
        }

        self.submit_and_close(window, cx);
    }

    fn clear_transient_feedback(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.feedback, QuickAddFeedback::Idle) {
            self.feedback = QuickAddFeedback::Idle;
            cx.notify();
        }
    }

    fn start_escape_confirm_timeout(&mut self, cx: &mut Context<Self>) {
        self.esc_confirm_generation += 1;
        let generation = self.esc_confirm_generation;
        self.feedback = QuickAddFeedback::EscConfirmPending { generation };
        cx.notify();

        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(StdDuration::from_secs(5))
                .await;

            let _ = view.update(cx, |this, cx| {
                if matches!(
                    this.feedback,
                    QuickAddFeedback::EscConfirmPending {
                        generation: current_generation
                    } if current_generation == generation
                ) {
                    this.feedback = QuickAddFeedback::Idle;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn handle_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_input(cx) {
            self.close_window(window, cx);
            return;
        }

        match self.feedback {
            QuickAddFeedback::EscConfirmPending { .. } => self.close_window(window, cx),
            QuickAddFeedback::Idle | QuickAddFeedback::EmptySubmitWarning => {
                self.start_escape_confirm_timeout(cx);
            }
        }
    }

    fn feedback_style(&self) -> Option<(Hsla, Hsla, Hsla, &'static str)> {
        match self.feedback {
            QuickAddFeedback::Idle => None,
            QuickAddFeedback::EmptySubmitWarning => Some((
                rgb(0xff4d4f).into(),
                rgb(0xfff1f0).into(),
                rgb(0xffccc7).into(),
                "没有输入内容，不能记录",
            )),
            QuickAddFeedback::EscConfirmPending { .. } => Some((
                rgb(0xfa8c16).into(),
                rgb(0xfff7e6).into(),
                rgb(0xffd591).into(),
                "再次按 Esc 关闭输入",
            )),
        }
    }

    fn render_feedback_message(&self) -> Option<impl IntoElement> {
        let (text_color, _, _, message) = self.feedback_style()?;

        Some(
            div()
                .text_sm()
                .text_color(text_color)
                .font_weight(FontWeight::MEDIUM)
                .child(message),
        )
    }

    fn submit_task(&mut self, cx: &mut Context<Self>) {
        let text = self.input_state.read(cx).text().to_string();
        if text.trim().is_empty() {
            return;
        }

        let (title, priority, tags, people) = parsing::parse_task_input(&text);
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

        let (content, tags, people) = parsing::parse_record_input(&text);
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

    fn render_submit_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (bg, hover, border) = match self.mode {
            InputMode::Task => (rgb(0xfff7e6), rgb(0xffe7ba), rgb(0xffbb96)),
            InputMode::Record => (rgb(0xe6f4ff), rgb(0xbae0ff), rgb(0x91caff)),
        };

        div()
            .id(format!("submit-{}", self.mode.label()))
            .flex()
            .justify_center()
            .items_center()
            .px(px(14.0))
            .py(px(8.0))
            .rounded(px(999.0))
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_color(rgb(0x0958d9))
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .cursor_pointer()
            .hover(|style| style.bg(hover))
            .child(self.mode.submit_label())
            .on_click(cx.listener(|this, _event: &ClickEvent, window, cx| {
                this.try_submit(window, cx);
            }))
    }

    fn render_mode_switch_button(&self, mode: InputMode, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(format!("switch-to-{}", mode.label()))
            .flex()
            .justify_center()
            .items_center()
            .w(px(36.0))
            .h(px(36.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(rgb(0xd9d9d9))
            .bg(rgb(0xffffff))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0xf5f5f5)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(0.0))
                    .child(
                        gpui_component::Icon::new(IconName::ArrowRight)
                            .size(px(15.0))
                            .text_color(rgb(0x262626)),
                    )
                    .child(
                        gpui_component::Icon::new(IconName::ArrowLeft)
                            .size(px(15.0))
                            .text_color(rgb(0x262626)),
                    ),
            )
            .on_click(cx.listener(move |this, _event: &ClickEvent, window, cx| {
                this.clear_transient_feedback(cx);
                this.set_mode(mode, window, cx);
            }))
    }

    fn render_mode_switcher(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let next_mode = match self.mode {
            InputMode::Task => InputMode::Record,
            InputMode::Record => InputMode::Task,
        };

        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(self.render_submit_button(cx))
            .child(self.render_mode_switch_button(next_mode, cx))
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

        let feedback_style = self.feedback_style();

        div()
            .size_full()
            .bg(rgb(0xfcfcfd))
            .p(px(14.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(|this, _action: &Escape, window, cx| {
                this.handle_escape(window, cx);
            }))
            .on_action(cx.listener(|this, _: &IndentInline, window, cx| {
                this.clear_transient_feedback(cx);
                this.toggle_mode(window, cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                if event.keystroke.key != "escape" {
                    this.clear_transient_feedback(cx);
                }
            }))
            .child(
                div()
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(
                        feedback_style
                            .map(|(_, _, border_color, _)| border_color)
                            .unwrap_or_else(transparent_black),
                    )
                    .bg(
                        feedback_style
                            .map(|(_, bg_color, _, _)| bg_color)
                            .unwrap_or_else(transparent_white),
                    )
                    .p(px(2.0))
                    .child(Input::new(&self.input_state)),
            )
            .children(self.render_feedback_message())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(self.render_mode_switcher(cx)),
            )
    }
}
