use crate::models::Record;
use crate::store::Store;
use crate::ui::parsing;
use crate::ui::record_detail_sidebar::{RecordDetailSidebar, SavePayload};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::button::Button;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::scroll::ScrollableElement;
use uuid::Uuid;

const NOTE_TITLE_LIMIT: usize = 24;
const NOTE_PREVIEW_LIMIT: usize = 44;

#[derive(Clone)]
struct PendingDeletion {
    id: Uuid,
    record_label: &'static str,
    display_title: String,
}

pub struct NotePanel {
    store: Store,
    notes: Vec<Record>,
    focus_handle: FocusHandle,
    input_state: Entity<InputState>,
    _subscription: Subscription,
    editing_note_id: Option<uuid::Uuid>,
    edit_input_state: Option<Entity<InputState>>,
    _edit_subscription: Option<Subscription>,
    _window_activation_subscription: Subscription,
    pending_deletion: Option<PendingDeletion>,
    record_detail_sidebar: Entity<RecordDetailSidebar>,
}

impl NotePanel {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入笔记内容，第一行自动作为标题，Enter 保存 | #标签 @人物")
        });

        let _subscription = cx.subscribe_in(
            &input_state,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.create_note(window, cx);
                }
            },
        );

        let mut panel = Self {
            store: store.clone(),
            notes: Vec::new(),
            focus_handle,
            input_state,
            _subscription,
            editing_note_id: None,
            edit_input_state: None,
            _edit_subscription: None,
            _window_activation_subscription: cx.observe_window_activation(
                window,
                |this, window, cx| {
                    if window.is_window_active() {
                        this.load_notes(cx);
                    }
                },
            ),
            pending_deletion: None,
            record_detail_sidebar: cx.new(|cx| RecordDetailSidebar::new(window, cx)),
        };

        let handle = cx.entity().clone();
        panel.record_detail_sidebar.update(cx, |sidebar, _cx| {
            sidebar.on_save(move |payload, cx| {
                handle.update(cx, |panel, cx| {
                    panel.handle_sidebar_save(&payload, cx);
                });
            });
        });
        let handle = cx.entity().clone();
        panel.record_detail_sidebar.update(cx, |sidebar, _cx| {
            sidebar.on_delete(move |record_id, cx| {
                if let Ok(record_id) = Uuid::parse_str(&record_id) {
                    handle.update(cx, |panel, cx| {
                        panel.request_delete_note(record_id, cx);
                    });
                }
            });
        });

        panel.load_notes(cx);
        panel
    }

    pub fn focus_primary_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_deletion.is_some()
            || self.editing_note_id.is_some()
            || self
                .record_detail_sidebar
                .read(cx)
                .current_record_id()
                .is_some()
        {
            self.focus_handle.focus(window, cx);
            return;
        }

        self.focus_handle.focus(window, cx);
        self.input_state.update(cx, |state, cx| {
            state.focus(window, cx);
        });
    }

    fn handle_sidebar_save(&mut self, payload: &SavePayload, cx: &mut Context<Self>) {
        if let Some(note) = self
            .notes
            .iter_mut()
            .find(|n| n.id.to_string() == payload.record_id)
        {
            note.title = payload.title.clone();
            note.content = payload.content.clone();
            note.updated_at = chrono::Utc::now();

            let updated_note = note.clone();
            let store = self.store.clone();
            cx.spawn(async move |_view, _cx| {
                if let Err(e) = store.update_record(updated_note).await {
                    eprintln!("[NotePanel] Failed to update note: {}", e);
                }
            })
            .detach();

            cx.notify();
        }
    }

    fn select_record(&mut self, record: &Record, window: &mut Window, cx: &mut Context<Self>) {
        self.record_detail_sidebar.update(cx, |sidebar, cx| {
            sidebar.show_record(record, window, cx);
        });
        cx.notify();
    }

    fn start_edit(&mut self, note: Record, window: &mut Window, cx: &mut Context<Self>) {
        self.editing_note_id = Some(note.id);
        let edit_input = cx.new(|cx| InputState::new(window, cx).placeholder("编辑笔记..."));
        // 将内容中的换行符替换为空格，避免 gpui_component::input::Input 遇到换行符时 crash
        let content = note.content.replace('\n', " ");
        edit_input.update(cx, |state, cx| {
            state.set_value(&content, window, cx);
        });

        let _edit_subscription = cx.subscribe_in(
            &edit_input,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.save_edit(window, cx);
                }
            },
        );

        self.edit_input_state = Some(edit_input);
        self._edit_subscription = Some(_edit_subscription);
        cx.notify();
    }

    fn save_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(note_id) = self.editing_note_id {
            if let Some(edit_input) = &self.edit_input_state {
                let new_content = edit_input.read(cx).text().to_string();
                if let Some(note) = self.notes.iter_mut().find(|n| n.id == note_id) {
                    note.content = new_content;
                    let updated_note = note.clone();
                    let store = self.store.clone();
                    cx.spawn(async move |_view, _cx| {
                        if let Err(e) = store.update_record(updated_note).await {
                            eprintln!("[NotePanel] Failed to update note: {}", e);
                        }
                    })
                    .detach();
                }
            }
        }
        self.cancel_edit(window, cx);
    }

    fn cancel_edit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.editing_note_id = None;
        self.edit_input_state = None;
        self._edit_subscription = None;
        cx.notify();
    }

    fn load_notes(&mut self, cx: &mut Context<Self>) {
        eprintln!("[NotePanel] load_notes called");
        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            let mut retries = 0;
            let notes = loop {
                eprintln!(
                    "[NotePanel] Fetching notes from store... (attempt {})",
                    retries + 1
                );
                match store.get_notes().await {
                    Ok(notes) => break notes,
                    Err(e) => {
                        eprintln!("[NotePanel] Failed to load notes: {}, retrying...", e);
                        retries += 1;
                        if retries >= 3 {
                            eprintln!("[NotePanel] Max retries reached, giving up");
                            break Vec::new();
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            };

            eprintln!("[NotePanel] Loaded {} notes", notes.len());
            let update_result = view.update(cx, |panel, cx| {
                panel.notes = notes;
                cx.notify();
                eprintln!(
                    "[NotePanel] Notes updated and notified, panel now has {} notes",
                    panel.notes.len()
                );
            });
            if let Err(e) = update_result {
                eprintln!("[NotePanel] Failed to update view: {:?}", e);
            }
        })
        .detach();
    }

    fn create_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input_state.read(cx).text().to_string();

        eprintln!("[NotePanel] create_note called with text: '{}'", text);

        if text.trim().is_empty() {
            eprintln!("[NotePanel] Text is empty, returning");
            return;
        }

        let (content, tags, people) = parsing::parse_record_input(&text);
        eprintln!(
            "[NotePanel] Parsed content: '{}', tags: {:?}, people: {:?}",
            content, tags, people
        );

        let mut note = Record::new_note(if content.is_empty() { text } else { content });
        note.tags = tags;
        note.persons = people;
        eprintln!(
            "[NotePanel] Created note with id: {}, tags: {:?}, persons: {:?}",
            note.id, note.tags, note.persons
        );

        // Clear input
        self.input_state.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });

        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            eprintln!("[NotePanel] Spawning create_record...");
            match store.create_record(note).await {
                Ok(_) => {
                    eprintln!("[NotePanel] create_record succeeded, scheduling load_notes");
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    let update_result = view.update(cx, |panel, cx| {
                        eprintln!("[NotePanel] About to call load_notes from create_note callback");
                        panel.load_notes(cx);
                        eprintln!("[NotePanel] load_notes called from callback");
                    });
                    if let Err(e) = update_result {
                        eprintln!("[NotePanel] Failed to update view: {:?}", e);
                    } else {
                        eprintln!("[NotePanel] View view succeeded");
                    }
                }
                Err(e) => eprintln!("[NotePanel] Failed to create note: {}", e),
            }
        })
        .detach();
    }

    fn request_delete_note(&mut self, note_id: Uuid, cx: &mut Context<Self>) {
        if let Some(note) = self.notes.iter().find(|n| n.id == note_id) {
            self.pending_deletion = Some(PendingDeletion {
                id: note_id,
                record_label: "记录",
                display_title: Self::get_note_display(note).0,
            });
            cx.notify();
        }
    }

    fn cancel_delete_confirmation(&mut self, cx: &mut Context<Self>) {
        self.pending_deletion = None;
        cx.notify();
    }

    fn confirm_delete_note(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_deletion.clone() else {
            return;
        };

        self.perform_delete_note(pending.id, true, cx);
    }

    fn perform_delete_note(
        &mut self,
        note_id: Uuid,
        clear_confirmation: bool,
        cx: &mut Context<Self>,
    ) {
        let store = self.store.clone();
        let note_id_string = note_id.to_string();
        cx.spawn(
            async move |view, cx| match store.delete_record(note_id).await {
                Ok(_) => {
                    view.update(cx, |panel, cx| {
                        if clear_confirmation {
                            panel.pending_deletion = None;
                        }
                        if panel.record_detail_sidebar.read(cx).current_record_id()
                            == Some(note_id_string.as_str())
                        {
                            panel.record_detail_sidebar.update(cx, |sidebar, cx| {
                                sidebar.dismiss(cx);
                            });
                        }
                        panel.load_notes(cx);
                    })
                    .ok();
                }
                Err(e) => eprintln!("[NotePanel] Failed to delete note: {}", e),
            },
        )
        .detach();
    }

    // 从 Record 获取显示标题和预览
    // 优先使用 title 字段，如果没有则从 content 提取
    fn get_note_display(note: &Record) -> (String, String) {
        let title = note
            .title
            .clone()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| {
                note.content
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .map(|line| line.trim().to_string())
                    .unwrap_or_else(|| "无标题".to_string())
            });

        let preview = note
            .content
            .lines()
            .skip(1)
            .find(|line| !line.trim().is_empty())
            .map(|line| line.trim().to_string())
            .unwrap_or_default();

        (
            Self::truncate_text(&title, NOTE_TITLE_LIMIT),
            Self::truncate_text(&preview, NOTE_PREVIEW_LIMIT),
        )
    }

    fn normalize_text(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn truncate_text(text: &str, limit: usize) -> String {
        let normalized = Self::normalize_text(text);
        if normalized.chars().count() <= limit {
            normalized
        } else {
            format!("{}...", normalized.chars().take(limit).collect::<String>())
        }
    }

    fn render_delete_confirmation(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pending = self.pending_deletion.as_ref()?;
        let title = pending.display_title.clone();
        let record_label = pending.record_label;

        Some(
            div()
                .id("note-delete-confirm-overlay")
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(0xf5f5f5))
                .on_click(cx.listener(|_this, _event: &ClickEvent, _window, cx| {
                    cx.stop_propagation();
                }))
                .child(
                    div()
                        .w(px(360.0))
                        .max_w(px(360.0))
                        .p(px(20.0))
                        .rounded(px(12.0))
                        .bg(rgb(0xffffff))
                        .border_1()
                        .border_color(rgb(0xe8e8e8))
                        .shadow_lg()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .cursor_default()
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("删除{}", record_label)),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0x666666))
                                .child(format!("确认删除“{}”？删除后无法恢复。", title)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x999999))
                                .child("按 Enter 确认，按 Esc 取消"),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(8.0))
                                .child(
                                    Button::new("note-delete-confirm-cancel")
                                        .child("取消")
                                        .on_click(cx.listener(|this, _event, _window, cx| {
                                            this.cancel_delete_confirmation(cx);
                                        })),
                                )
                                .child(
                                    Button::new("note-delete-confirm-submit")
                                        .child("确认删除")
                                        .text_color(rgb(0xff4d4f))
                                        .on_click(cx.listener(|this, _event, _window, cx| {
                                            this.confirm_delete_note(cx);
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}

impl Render for NotePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.pending_deletion.is_some() {
            self.focus_handle.focus(window, cx);
        }

        eprintln!("[NotePanel] Rendering {} notes", self.notes.len());

        let sidebar_task_id = self
            .record_detail_sidebar
            .read(cx)
            .current_record_id()
            .map(|s| s.to_string());

        div()
            .size_full()
            .flex()
            .flex_row()
            .relative()
            .track_focus(&self.focus_handle(cx))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.pending_deletion.is_none() {
                    return;
                }

                match event.keystroke.key.as_str() {
                    "enter" => {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.confirm_delete_note(cx);
                    }
                    "escape" => {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.cancel_delete_confirmation(cx);
                    }
                    _ => {}
                }
            }))
            .child(
                div()
                    .id("note-panel-main")
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .p(px(16.0))
                    .on_click(cx.listener(|this, _event: &ClickEvent, window, cx| {
                        if this.record_detail_sidebar.read(cx).current_record_id().is_some() {
                            this.record_detail_sidebar.update(cx, |sidebar, cx| {
                                sidebar.close(window, cx);
                            });
                        }
                    }))
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("记录 ({})", self.notes.len()))
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
                                            this.create_note(window, cx);
                                        }
                                    }))
                                    .child(Input::new(&self.input_state))
                            )
                            .child(
                                Button::new("add-btn")
                                    .child("添加")
                                    .on_click(cx.listener(|this, _event: &ClickEvent, window, cx| {
                                        this.create_note(window, cx);
                                    }))
                            )
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x888888))
                            .child("输入记录内容，第一行自动作为标题，Enter 保存 | #标签 @人物")
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .child(
                                div()
                                    .id("note-list")
                                    .size_full()
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .pr(px(16.0))
                                    .overflow_y_scrollbar()
                                    .children(self.notes.clone().into_iter().enumerate().map(|(idx, note)| {
                                let note_id = note.id;
                                let is_editing = self.editing_note_id == Some(note_id);
                                let is_selected = sidebar_task_id.as_ref() == Some(&note_id.to_string());
                                let (title, preview) = Self::get_note_display(&note);

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
                                    .id(idx)
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .px(px(12.0))
                                    .py(px(8.0))
                                    .rounded(px(6.0))
                                    .bg(if is_selected { rgb(0xe6f7ff) } else { rgb(0xe8e8e8) })
                                    .border_1()
                                    .border_color(if is_selected { rgb(0x1890ff) } else { rgb(0xe8e8e8) })
                                    .text_color(rgb(0x000000))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(if is_selected { rgb(0xe6f7ff) } else { rgb(0xf0f0f0) }))
                                    .on_click(cx.listener({
                                        let note = note.clone();
                                        move |this, _event: &ClickEvent, window, cx| {
                                            this.select_record(&note, window, cx);
                                            cx.stop_propagation();
                                        }
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(8.0))
                                            .items_center()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .overflow_hidden()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(rgb(0x000000))
                                                    .child(title)
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .gap(px(4.0))
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .cursor_pointer()
                                                            .px(px(4.0))
                                                            .text_color(rgb(0x888888))
                                                            .hover(|style| style.text_color(rgb(0x1890ff)))
                                                            .child("✎")
                                                            .id(("edit", idx))
                                                            .on_click(cx.listener({
                                                                let note = note.clone();
                                                                move |this, _event: &ClickEvent, window, cx| {
                                                                    this.start_edit(note.clone(), window, cx);
                                                                    cx.stop_propagation();
                                                                }
                                                            }))
                                                    )
                                                    .child(
                                                        div()
                                                            .cursor_pointer()
                                                            .px(px(4.0))
                                                            .text_color(rgb(0x888888))
                                                            .hover(|style| style.text_color(rgb(0xff4d4f)))
                                                            .child("×")
                                                            .id(("delete", idx))
                                                            .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                                                                this.request_delete_note(note_id, cx);
                                                                cx.stop_propagation();
                                                            }))
                                                    )
                                            )
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(0x444444))
                                            .line_height(relative(1.35))
                                            .child(if preview.is_empty() { "...".to_string() } else { preview })
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .justify_end()
                                            .items_center()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x999999))
                                                    .child(format!("创建于: {}", note.created_at.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M")))
                                            )
                                    )
                                    .when(!note.tags.is_empty(), |el| {
                                        el.child(
                                            div()
                                                .flex()
                                                .gap(px(6.0))
                                                .flex_wrap()
                                                .children(note.tags.iter().enumerate().map(|(tag_idx, tag)| {
                                                    div()
                                                        .id(("note-tag", tag_idx))
                                                        .px(px(6.0))
                                                        .py(px(2.0))
                                                        .rounded(px(4.0))
                                                        .bg(rgb(0xf5f5f5))
                                                        .text_xs()
                                                        .text_color(rgb(0x595959))
                                                        .child(format!("#{}", tag))
                                                }))
                                        )
                                    })
                                    .when(!note.persons.is_empty(), |el| {
                                        el.child(
                                            div()
                                                .flex()
                                                .gap(px(6.0))
                                                .flex_wrap()
                                                .children(note.persons.iter().enumerate().map(|(person_idx, person)| {
                                                    div()
                                                        .id(("note-person", person_idx))
                                                        .px(px(6.0))
                                                        .py(px(2.0))
                                                        .rounded(px(4.0))
                                                        .bg(rgb(0xe6f7ff))
                                                        .text_xs()
                                                        .text_color(rgb(0x1890ff))
                                                        .child(format!("@{}", person))
                                                }))
                                        )
                                    })
                                    .into_any_element()
                            }))
                        )
                    )
            )
            .child(self.record_detail_sidebar.clone())
            .children(self.render_delete_confirmation(cx))
    }
}

impl Focusable for NotePanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
