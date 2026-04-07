use crate::models::Record;
use crate::store::Store;
use crate::ui::parsing;
use gpui::*;
use gpui_component::input::{Escape, IndentInline, Input, InputEvent, InputState};
use gpui_component::IconName;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration as StdDuration;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputMode {
    Task,
    Record,
}

impl Default for InputMode {
    fn default() -> Self {
        Self::Record
    }
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
            InputMode::Task => {
                "输入任务标题，Enter 换行添加正文 (Cmd+Enter 保存, Shift+Cmd+Enter 打开任务)"
            }
            InputMode::Record => {
                "输入记录，Enter 换行后首行作为标题 (Cmd+Enter 保存, Shift+Cmd+Enter 打开记录)"
            }
        }
    }

    fn destination(&self) -> QuickAddDestination {
        match self {
            InputMode::Task => QuickAddDestination::Tasks,
            InputMode::Record => QuickAddDestination::Records,
        }
    }
}

pub fn quick_add_window_size() -> Size<Pixels> {
    size(px(520.0), px(244.0))
}

fn quick_add_window_size_for_rows(rows: usize) -> Size<Pixels> {
    let extra_rows = rows.saturating_sub(1).min(5) as f32;
    size(px(520.0), px(244.0 + extra_rows * 28.0))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum QuickAddDestination {
    Main,
    Tasks,
    Records,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum QuickAddSessionStatus {
    #[default]
    Closed,
    Visible,
    Dormant,
}

#[derive(Clone, Debug, Default)]
pub struct QuickAddSessionController {
    pub handle: Option<AnyWindowHandle>,
    pub draft_text: String,
    pub mode: InputMode,
    pub status: QuickAddSessionStatus,
    pub hide_app_on_close: bool,
}

impl QuickAddSessionController {
    pub fn has_draft(&self) -> bool {
        !self.draft_text.trim().is_empty()
    }

    pub fn mark_visible(&mut self) {
        self.status = QuickAddSessionStatus::Visible;
    }

    pub fn clear(&mut self) {
        self.handle = None;
        self.draft_text.clear();
        self.mode = InputMode::Record;
        self.status = QuickAddSessionStatus::Closed;
        self.hide_app_on_close = false;
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum QuickAddFeedback {
    Idle,
    EmptySubmitWarning,
    EscConfirmPending { generation: u64 },
    HotkeyDraftProtected,
}

pub struct QuickAddWindow {
    store: Store,
    session: Rc<RefCell<QuickAddSessionController>>,
    open_destination: Arc<dyn Fn(QuickAddDestination, &mut App)>,
    input_state: Entity<InputState>,
    _subscription: Subscription,
    mode: InputMode,
    focus_handle: FocusHandle,
    feedback: QuickAddFeedback,
    esc_confirm_generation: u64,
    pub hide_app_on_close: bool,
}

impl QuickAddWindow {
    pub fn new(
        store: Store,
        session: Rc<RefCell<QuickAddSessionController>>,
        open_destination: Arc<dyn Fn(QuickAddDestination, &mut App)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let snapshot = session.borrow().clone();
        let mode = snapshot.mode;
        let initial_text = snapshot.draft_text;

        let input_state = cx.new(|cx| {
            let mut input = InputState::new(window, cx)
                .multi_line(true)
                .auto_grow(1, 6)
                .placeholder(mode.placeholder());
            if !initial_text.is_empty() {
                input.set_value(initial_text, window, cx);
            }
            input
        });

        let _subscription = cx.subscribe_in(
            &input_state,
            window,
            |this, _state, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { secondary } => {
                    if *secondary {
                        let draft_text = this.session.borrow().draft_text.clone();
                        if window.modifiers().shift {
                            this.try_submit_and_open_with_text(
                                draft_text,
                                this.mode.destination(),
                                window,
                                cx,
                            );
                        } else {
                            this.try_submit_with_text(draft_text, window, cx);
                        }
                    }
                }
                InputEvent::Change => {
                    this.sync_session_state(cx);
                    this.clear_transient_feedback(cx);
                    this.sync_window_size(window, cx);
                }
                InputEvent::Focus | InputEvent::Blur => {}
            },
        );

        session.borrow_mut().mark_visible();

        let view = Self {
            store,
            session,
            open_destination,
            input_state,
            _subscription,
            mode,
            focus_handle,
            feedback: QuickAddFeedback::Idle,
            esc_confirm_generation: 0,
            hide_app_on_close: false,
        };
        view.sync_window_size(window, cx);
        view
    }

    pub fn show_hotkey_protection(&mut self, cx: &mut Context<Self>) {
        self.feedback = QuickAddFeedback::HotkeyDraftProtected;
        cx.notify();
    }

    fn set_mode(&mut self, mode: InputMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.clear_transient_feedback(cx);
            self.mode = mode;
            self.input_state.update(cx, |input, cx| {
                input.set_placeholder(mode.placeholder(), window, cx);
            });
            self.sync_session_state(cx);
            self.sync_window_size(window, cx);
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

    fn visible_input_rows(&self, cx: &Context<Self>) -> usize {
        self.input_text(cx).split('\n').count().clamp(1, 6)
    }

    fn sync_window_size(&self, window: &mut Window, cx: &Context<Self>) {
        window.resize(quick_add_window_size_for_rows(self.visible_input_rows(cx)));
    }

    fn input_text(&self, cx: &Context<Self>) -> String {
        self.input_state.read(cx).text().to_string()
    }

    fn has_input(&self, cx: &Context<Self>) -> bool {
        !self.input_text(cx).trim().is_empty()
    }

    fn sync_session_state(&self, cx: &Context<Self>) {
        let mut session = self.session.borrow_mut();
        session.draft_text = self.input_text(cx);
        session.mode = self.mode;
        session.status = QuickAddSessionStatus::Visible;
    }

    fn clear_session(&self) {
        self.session.borrow_mut().clear();
    }

    fn mark_session_dormant(&self, cx: &Context<Self>) {
        let mut session = self.session.borrow_mut();
        session.handle = None;
        session.draft_text = self.input_text(cx);
        session.mode = self.mode;
        session.status = QuickAddSessionStatus::Dormant;
    }

    fn submit_text(&mut self, text: &str, cx: &mut Context<Self>) {
        match self.mode {
            InputMode::Task => self.submit_task_text(text, cx),
            InputMode::Record => self.submit_record_text(text, cx),
        }
    }

    fn remove_window(&mut self, window: &mut Window) {
        self.session.borrow_mut().handle = None;
        window.remove_window();
    }

    fn close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_session();
        let hide = self.hide_app_on_close;
        self.remove_window(window);
        if hide {
            cx.hide();
        }
    }

    fn close_window_preserving_session(&mut self, window: &mut Window) {
        self.remove_window(window);
    }

    fn submit_text_and_close(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_transient_feedback(cx);
        self.submit_text(text, cx);
        self.close_window(window, cx);
    }

    fn try_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input_text(cx);
        self.try_submit_with_text(text, window, cx);
    }

    fn try_submit_with_text(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        if text.trim().is_empty() {
            self.feedback = QuickAddFeedback::EmptySubmitWarning;
            cx.notify();
            return;
        }

        self.submit_text_and_close(&text, window, cx);
    }

    fn try_submit_and_open(
        &mut self,
        destination: QuickAddDestination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.input_text(cx);
        self.try_submit_and_open_with_text(text, destination, window, cx);
    }

    fn try_submit_and_open_with_text(
        &mut self,
        text: String,
        destination: QuickAddDestination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if text.trim().is_empty() {
            self.feedback = QuickAddFeedback::EmptySubmitWarning;
            cx.notify();
            return;
        }

        self.clear_transient_feedback(cx);
        self.submit_text(&text, cx);
        self.clear_session();
        self.remove_window(window);
        (self.open_destination)(destination, cx);
    }

    fn open_panel_without_submit(
        &mut self,
        destination: QuickAddDestination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_transient_feedback(cx);
        if self.has_input(cx) {
            self.mark_session_dormant(cx);
        } else {
            self.clear_session();
        }
        self.close_window_preserving_session(window);
        (self.open_destination)(destination, cx);
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
            QuickAddFeedback::Idle
            | QuickAddFeedback::EmptySubmitWarning
            | QuickAddFeedback::HotkeyDraftProtected => self.start_escape_confirm_timeout(cx),
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
            QuickAddFeedback::HotkeyDraftProtected => Some((
                rgb(0x0958d9).into(),
                rgb(0xe6f4ff).into(),
                rgb(0x91caff).into(),
                "已有草稿，按 Esc 关闭或按 Cmd+2 / Cmd+3 查看对应面板",
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

    fn render_shortcut_hints(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_wrap()
            .gap(px(12.0))
            .text_xs()
            .text_color(rgb(0x8c8c8c))
            .child("Enter 换行")
            .child("Cmd+Enter 保存")
            .child("Shift+Cmd+Enter 打开对应面板")
            .child("Cmd+2 查看任务")
            .child("Cmd+3 查看记录")
            .child("Tab 切换模式")
            .child("Esc 关闭")
    }

    fn submit_task_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if text.trim().is_empty() {
            return;
        }

        let parsed = parsing::parse_task_draft(text);
        let mut task = Record::new_task(parsed.title, parsed.content, parsed.priority);

        for tag in parsed.tags {
            task.tags.push(tag);
        }
        for person in parsed.people {
            task.persons.push(person);
        }

        let store = self.store.clone();
        cx.spawn(async move |_view, _cx| {
            if let Err(e) = store.create_record(task).await {
                eprintln!("[QuickAdd] Failed to create task: {}", e);
            }
        })
        .detach();
    }

    fn submit_record_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if text.trim().is_empty() {
            return;
        }

        let parsed = parsing::parse_record_draft(text);
        let mut record = Record::new_note_with_title(parsed.title, parsed.content);

        for tag in parsed.tags {
            record.tags.push(tag);
        }
        for person in parsed.people {
            record.persons.push(person);
        }

        let store = self.store.clone();
        cx.spawn(async move |_view, _cx| {
            if let Err(e) = store.create_record(record).await {
                eprintln!("[QuickAdd] Failed to create record: {}", e);
            }
        })
        .detach();
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
            .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                if event.modifiers().shift {
                    this.try_submit_and_open(this.mode.destination(), window, cx);
                } else {
                    this.try_submit(window, cx);
                }
            }))
    }

    fn render_mode_switch_button(
        &self,
        mode: InputMode,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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

impl Focusable for QuickAddWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for QuickAddWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.input_state
            .update(cx, |input, cx| input.focus(window, cx));

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
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let modifiers = event.keystroke.modifiers;
                let key = event.keystroke.key.as_str();

                if key == "enter" && modifiers.platform && modifiers.shift {
                    window.prevent_default();
                    cx.stop_propagation();
                    let draft_text = this.session.borrow().draft_text.clone();
                    this.try_submit_and_open_with_text(
                        draft_text,
                        this.mode.destination(),
                        window,
                        cx,
                    );
                    return;
                }

                if modifiers.platform {
                    match key {
                        "2" => {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.open_panel_without_submit(QuickAddDestination::Tasks, window, cx);
                            return;
                        }
                        "3" => {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.open_panel_without_submit(
                                QuickAddDestination::Records,
                                window,
                                cx,
                            );
                            return;
                        }
                        _ => {}
                    }
                }

                if key != "escape" {
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
                    .bg(feedback_style
                        .map(|(_, bg_color, _, _)| bg_color)
                        .unwrap_or_else(transparent_white))
                    .p(px(2.0))
                    .child(Input::new(&self.input_state)),
            )
            .children(self.render_feedback_message())
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_start()
                    .child(self.render_mode_switcher(cx)),
            )
            .child(self.render_shortcut_hints())
    }
}
