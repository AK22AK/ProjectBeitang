use crate::models::Record;
use crate::store::Store;
use crate::ui::parsing;
use crate::ui::record_detail_sidebar::{RecordDetailSidebar, SavePayload};
use gpui::*;
use gpui::prelude::FluentBuilder as _;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::button::Button;
use gpui_component::scroll::ScrollableElement;
use uuid::Uuid;

pub struct NotePanel {
    store: Store,
    notes: Vec<Record>,
    input_state: Entity<InputState>,
    _subscription: Subscription,
    editing_note_id: Option<uuid::Uuid>,
    edit_input_state: Option<Entity<InputState>>,
    _edit_subscription: Option<Subscription>,
    _window_activation_subscription: Subscription,
    record_detail_sidebar: Entity<RecordDetailSidebar>,
}

impl NotePanel {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
            input_state,
            _subscription,
            editing_note_id: None,
            edit_input_state: None,
            _edit_subscription: None,
            _window_activation_subscription: cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.load_notes(cx);
                }
            }),
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

        panel.load_notes(cx);
        panel
    }

    fn handle_sidebar_save(&mut self, payload: &SavePayload, cx: &mut Context<Self>) {
        if let Some(note) = self.notes.iter_mut().find(|n| n.id.to_string() == payload.record_id) {
            note.title = payload.title.clone();
            note.content = payload.content.clone();
            note.updated_at = chrono::Utc::now();

            let updated_note = note.clone();
            let store = self.store.clone();
            cx.spawn(async move |_view, _cx| {
                if let Err(e) = store.update_record(updated_note).await {
                    eprintln!("[NotePanel] Failed to update note: {}", e);
                }
            }).detach();

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
        let edit_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("编辑笔记...")
        });
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
                    }).detach();
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
                eprintln!("[NotePanel] Fetching notes from store... (attempt {})", retries + 1);
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
                eprintln!("[NotePanel] Notes updated and notified, panel now has {} notes", panel.notes.len());
            });
            if let Err(e) = update_result {
                eprintln!("[NotePanel] Failed to update view: {:?}", e);
            }
        }).detach();
    }

    fn create_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input_state.read(cx).text().to_string();

        eprintln!("[NotePanel] create_note called with text: '{}'", text);

        if text.trim().is_empty() {
            eprintln!("[NotePanel] Text is empty, returning");
            return;
        }

        let (content, tags, people) = parsing::parse_record_input(&text);
        eprintln!("[NotePanel] Parsed content: '{}', tags: {:?}, people: {:?}", content, tags, people);

        let mut note = Record::new_note(if content.is_empty() { text } else { content });
        note.tags = tags;
        note.persons = people;
        eprintln!("[NotePanel] Created note with id: {}, tags: {:?}, persons: {:?}", note.id, note.tags, note.persons);

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
        }).detach();
    }

    fn delete_note(&mut self, note_id: Uuid, cx: &mut Context<Self>) {
        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            match store.delete_record(note_id).await {
                Ok(_) => {
                    view.update(cx, |panel, cx| {
                        panel.load_notes(cx);
                    }).ok();
                }
                Err(e) => eprintln!("[NotePanel] Failed to delete note: {}", e),
            }
        }).detach();
    }

    // 从 Record 获取显示标题和预览
    // 优先使用 title 字段，如果没有则从 content 提取
    fn get_note_display(note: &Record) -> (String, String) {
        // 标题：优先使用 title 字段
        let title = note.title.clone().unwrap_or_else(|| {
            // 如果没有 title，从 content 第一行提取
            note.content.lines()
                .find(|line| !line.trim().is_empty())
                .map(|line| line.trim().to_string())
                .unwrap_or_else(|| "无标题".to_string())
        });

        // 预览：获取第二行非空内容作为预览
        let preview = note.content.lines()
            .skip(1)
            .find(|line| !line.trim().is_empty())
            .map(|line| line.trim().to_string())
            .unwrap_or_default();

        (title, preview)
    }
}

impl Render for NotePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        eprintln!("[NotePanel] Rendering {} notes", self.notes.len());

        let sidebar_task_id = self.record_detail_sidebar.read(cx).current_record_id().map(|s| s.to_string());

        div()
            .size_full()
            .flex()
            .flex_row()
            .relative()
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
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(rgb(0x000000))
                                                    .child(title)
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
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
                                                                this.delete_note(note_id, cx);
                                                                cx.stop_propagation();
                                                            }))
                                                    )
                                            )
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .justify_between()
                                            .items_center()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(rgb(0x444444))
                                                    .child(if preview.is_empty() { "...".to_string() } else { preview })
                                            )
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
    }
}
