use crate::file_dialog::{pick_image_files, ParentWindowHint};
use crate::models::Record;
use crate::store::Store;
use crate::ui::attachment_draft::{
    attachment_lightbox_size, attachment_preview_size, format_attachment_meta,
    prepare_pending_attachments, PendingAttachment,
};
use crate::ui::parsing;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::input::{Escape, IndentInline, Input, InputEvent, InputState, Paste};
use gpui_component::IconName;
use gpui_component::{h_flex, v_flex};
use std::cell::RefCell;
use std::path::PathBuf;
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

fn quick_add_window_size_for_content(
    input_rows: usize,
    attachment_rows: usize,
    status_lines: usize,
) -> Size<Pixels> {
    let input_extra_rows = input_rows.saturating_sub(1).min(5) as f32;
    let attachment_extra_height = attachment_rows as f32 * 28.0;
    let status_extra_height = status_lines as f32 * 22.0;
    size(
        px(520.0),
        px(244.0 + input_extra_rows * 28.0 + attachment_extra_height + status_extra_height),
    )
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
    pub pending_attachments: Vec<PendingAttachment>,
    pub mode: InputMode,
    pub status: QuickAddSessionStatus,
    pub hide_app_on_close: bool,
}

impl QuickAddSessionController {
    pub fn has_draft(&self) -> bool {
        !self.draft_text.trim().is_empty() || !self.pending_attachments.is_empty()
    }

    pub fn mark_visible(&mut self) {
        self.status = QuickAddSessionStatus::Visible;
    }

    pub fn clear(&mut self) {
        self.handle = None;
        self.draft_text.clear();
        self.pending_attachments.clear();
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
    pending_attachments: Vec<PendingAttachment>,
    active_attachment_preview: Option<PendingAttachment>,
    attachments_loading: bool,
    attachment_error: Option<String>,
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
        let pending_attachments = snapshot.pending_attachments;

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
            pending_attachments,
            active_attachment_preview: None,
            attachments_loading: false,
            attachment_error: None,
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
        window.resize(quick_add_window_size_for_content(
            self.visible_input_rows(cx),
            self.visible_attachment_rows(),
            self.visible_status_lines(),
        ));
    }

    fn input_text(&self, cx: &Context<Self>) -> String {
        self.input_state.read(cx).text().to_string()
    }

    fn has_input(&self, cx: &Context<Self>) -> bool {
        !self.input_text(cx).trim().is_empty()
    }

    fn has_draft(&self, cx: &Context<Self>) -> bool {
        self.has_input(cx) || !self.pending_attachments.is_empty()
    }

    fn pending_attachment_paths(&self) -> Vec<std::path::PathBuf> {
        self.pending_attachments
            .iter()
            .map(|attachment| attachment.path.clone())
            .collect()
    }

    fn visible_attachment_rows(&self) -> usize {
        if self.pending_attachments.is_empty() {
            0
        } else {
            self.pending_attachments.len().div_ceil(12)
        }
    }

    fn visible_status_lines(&self) -> usize {
        usize::from(self.attachments_loading) + usize::from(self.attachment_error.is_some())
    }

    fn sync_session_state(&self, cx: &Context<Self>) {
        let mut session = self.session.borrow_mut();
        session.draft_text = self.input_text(cx);
        session.pending_attachments = self.pending_attachments.clone();
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
        session.pending_attachments = self.pending_attachments.clone();
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

        if self.attachments_loading {
            self.attachment_error = Some("图片仍在处理中，请稍候".to_string());
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

        if self.attachments_loading {
            self.attachment_error = Some("图片仍在处理中，请稍候".to_string());
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
        if self.has_draft(cx) {
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
        if !self.has_draft(cx) {
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
        let task_id = task.id;
        let pending_paths = self.pending_attachment_paths();

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
            } else if !pending_paths.is_empty() {
                if let Err(e) = store
                    .enqueue_record_attachment_import(task_id, pending_paths)
                    .await
                {
                    eprintln!("[QuickAdd] Failed to import task attachments: {}", e);
                }
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
        let record_id = record.id;
        let pending_paths = self.pending_attachment_paths();

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
            } else if !pending_paths.is_empty() {
                if let Err(e) = store
                    .enqueue_record_attachment_import(record_id, pending_paths)
                    .await
                {
                    eprintln!("[QuickAdd] Failed to import record attachments: {}", e);
                }
            }
        })
        .detach();
    }

    fn import_pending_attachments(&mut self, window: &Window, cx: &mut Context<Self>) {
        let picker = pick_image_files(ParentWindowHint::from_window(window));
        cx.spawn(async move |view, cx| {
            let Some(paths) = picker.await else {
                return;
            };

            let _ = view.update(cx, |this, cx| {
                this.append_pending_attachment_paths(paths, cx);
            });
        })
        .detach();
    }

    fn append_pending_attachment_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.attachments_loading = true;
        self.attachment_error = None;
        cx.notify();

        cx.spawn(async move |view, cx| {
            let (tx, rx) = async_channel::bounded(1);
            std::thread::spawn(move || {
                let result = prepare_pending_attachments(paths);
                let _ = tx.send_blocking(result);
            });

            let result = rx
                .recv()
                .await
                .map_err(|err| format!("图片处理任务失败: {}", err))
                .and_then(|result| result);

            let _ = view.update(cx, |this, cx| {
                this.attachments_loading = false;
                match result {
                    Ok(mut attachments) => {
                        this.pending_attachments.append(&mut attachments);
                        this.attachment_error = None;
                        this.sync_session_state(cx);
                    }
                    Err(err) => {
                        this.attachment_error = Some(err);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn paste_pending_attachments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(clipboard) = cx.read_from_clipboard() else {
            return;
        };
        if !crate::clipboard_attachment::clipboard_has_image_candidate(&clipboard) {
            return;
        }

        window.prevent_default();
        cx.stop_propagation();
        self.attachments_loading = true;
        self.attachment_error = None;
        cx.notify();

        cx.spawn(async move |view, cx| {
            let (tx, rx) = async_channel::bounded(1);
            std::thread::spawn(move || {
                let result =
                    crate::clipboard_attachment::prepare_pending_attachments_from_clipboard(
                        &clipboard,
                    );
                let _ = tx.send_blocking(result);
            });

            let result = rx
                .recv()
                .await
                .map_err(|err| format!("剪贴板图片处理任务失败: {}", err))
                .and_then(|result| result);

            let _ = view.update(cx, |this, cx| match result {
                Ok(mut attachments) if !attachments.is_empty() => {
                    this.attachments_loading = false;
                    this.pending_attachments.append(&mut attachments);
                    this.attachment_error = None;
                    this.sync_session_state(cx);
                    cx.notify();
                }
                Ok(_) => {
                    this.attachments_loading = false;
                    this.attachment_error = None;
                    cx.notify();
                }
                Err(err) => {
                    this.attachments_loading = false;
                    this.attachment_error = Some(err);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn remove_pending_attachment(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx < self.pending_attachments.len() {
            let removed = self.pending_attachments.remove(idx);
            if self
                .active_attachment_preview
                .as_ref()
                .is_some_and(|preview| preview.path == removed.path)
            {
                self.active_attachment_preview = None;
            }
            self.sync_session_state(cx);
            cx.notify();
        }
    }

    fn open_pending_attachment_preview(
        &mut self,
        preview: PendingAttachment,
        cx: &mut Context<Self>,
    ) {
        self.active_attachment_preview = None;
        match crate::system_preview::open_path(&preview.path) {
            Ok(()) => {
                self.attachment_error = None;
            }
            Err(err) => {
                self.attachment_error = Some(err);
            }
        }
        cx.notify();
    }

    fn close_pending_attachment_preview(&mut self, cx: &mut Context<Self>) {
        self.active_attachment_preview = None;
        cx.notify();
    }

    fn render_pending_attachment_card(
        &self,
        idx: usize,
        attachment: &PendingAttachment,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (preview_width, preview_height) = attachment_preview_size(attachment);
        let can_preview = attachment.preview_image.is_some();
        let preview_attachment = attachment.clone();

        h_flex()
            .id(("quick-add-pending-attachment", idx))
            .gap(px(4.0))
            .items_center()
            .px(px(4.0))
            .py(px(4.0))
            .border_1()
            .border_color(rgb(0xf0f0f0))
            .rounded(px(999.0))
            .bg(rgb(0xfcfcfc))
            .when(can_preview, |el| el.cursor_pointer())
            .when(can_preview, |el| {
                el.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                        this.open_pending_attachment_preview(preview_attachment.clone(), cx);
                        cx.stop_propagation();
                    }),
                )
            })
            .child(
                attachment
                    .preview_image
                    .clone()
                    .map(|image| {
                        div()
                            .w(px(16.0))
                            .h(px(16.0))
                            .rounded(px(4.0))
                            .bg(rgb(0xf5f5f5))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(img(image).w(preview_width).h(preview_height))
                            .into_any_element()
                    })
                    .unwrap_or_else(|| {
                        div()
                            .w(px(16.0))
                            .h(px(16.0))
                            .rounded(px(4.0))
                            .bg(rgb(0xf5f5f5))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .text_color(rgb(0x999999))
                            .child("图")
                            .into_any_element()
                    }),
            )
            .child(
                div()
                    .cursor_pointer()
                    .px(px(2.0))
                    .text_xs()
                    .text_color(rgb(0x999999))
                    .hover(|style| style.text_color(rgb(0xff4d4f)))
                    .child("×")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                            this.remove_pending_attachment(idx, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_pending_attachment_lightbox(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let preview = self.active_attachment_preview.as_ref()?;
        let image = preview.preview_image.clone()?;
        let meta = format_attachment_meta(preview);
        let (lightbox_width, lightbox_height) = attachment_lightbox_size(preview);

        Some(
            div()
                .id("quick-add-pending-attachment-lightbox")
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba(0x00000061))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                        this.close_pending_attachment_preview(cx);
                        cx.stop_propagation();
                    }),
                )
                .child(
                    v_flex()
                        .w(px(960.0))
                        .max_w(relative(0.9))
                        .gap(px(12.0))
                        .p(px(16.0))
                        .rounded(px(14.0))
                        .bg(rgb(0xffffff))
                        .border_1()
                        .border_color(rgb(0xe8e8e8))
                        .shadow_lg()
                        .cursor_default()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_this, _event: &MouseDownEvent, _window, cx| {
                                cx.stop_propagation();
                            }),
                        )
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .gap(px(12.0))
                                .child(
                                    v_flex()
                                        .gap(px(4.0))
                                        .min_w(px(0.0))
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(rgb(0x262626))
                                                .child(preview.file_name.clone()),
                                        )
                                        .child(
                                            div().text_sm().text_color(rgb(0x666666)).child(meta),
                                        ),
                                )
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .px(px(8.0))
                                        .py(px(4.0))
                                        .rounded(px(8.0))
                                        .hover(|style| style.bg(rgb(0xf5f5f5)))
                                        .child("关闭")
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(
                                                |this, _event: &MouseDownEvent, _window, cx| {
                                                    this.close_pending_attachment_preview(cx);
                                                    cx.stop_propagation();
                                                },
                                            ),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .min_h(px(240.0))
                                .max_h(px(760.0))
                                .py(px(8.0))
                                .rounded(px(10.0))
                                .bg(rgb(0xf5f5f5))
                                .flex()
                                .items_center()
                                .justify_center()
                                .overflow_hidden()
                                .child(img(image).w(lightbox_width).h(lightbox_height)),
                        ),
                )
                .into_any_element(),
        )
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

    fn render_inline_attachment_trigger(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("quick-add-inline-attachment-trigger")
            .flex()
            .items_center()
            .justify_center()
            .w(px(28.0))
            .h(px(28.0))
            .rounded(px(8.0))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0xf5f5f5)))
            .child(
                gpui_component::Icon::new(IconName::Plus)
                    .size(px(14.0))
                    .text_color(rgb(0x595959)),
            )
            .on_click(cx.listener(|this, _event: &ClickEvent, window, cx| {
                this.import_pending_attachments(window, cx);
                cx.stop_propagation();
            }))
    }
}

impl Focusable for QuickAddWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for QuickAddWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_window_size(window, cx);
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
            .capture_action(cx.listener(|this, _action: &Paste, window, cx| {
                this.paste_pending_attachments(window, cx);
            }))
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
                    .w_full()
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
                    .child(
                        h_flex()
                            .w_full()
                            .items_start()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(Input::new(&self.input_state).flex_1()),
                            )
                            .child(self.render_inline_attachment_trigger(cx)),
                    ),
            )
            .when(self.attachments_loading, |el| {
                el.child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x999999))
                        .child("正在处理图片…"),
                )
            })
            .when_some(self.attachment_error.clone(), |el, err| {
                el.child(div().text_sm().text_color(rgb(0xff4d4f)).child(err))
            })
            .when(!self.pending_attachments.is_empty(), |el| {
                el.child(
                    h_flex().gap(px(8.0)).flex_wrap().children(
                        self.pending_attachments
                            .iter()
                            .enumerate()
                            .map(|(idx, attachment)| {
                                self.render_pending_attachment_card(idx, attachment, cx)
                            }),
                    ),
                )
            })
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
            .when_some(
                self.render_pending_attachment_lightbox(cx),
                |el, overlay| el.child(overlay),
            )
    }
}
