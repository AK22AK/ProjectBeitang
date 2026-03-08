use crate::models::Record;
use crate::store::Store;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::button::Button;
use uuid::Uuid;

pub struct NotePanel {
    store: Store,
    notes: Vec<Record>,
    input_state: Entity<InputState>,
    _subscription: Subscription,
}

impl NotePanel {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入笔记内容，第一行自动作为标题，Enter 保存")
        });

        let _subscription = cx.subscribe_in(
            &input_state,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => {
                        this.create_note(window, cx);
                    }
                    _ => {}
                }
            },
        );

        let mut panel = Self {
            store,
            notes: Vec::new(),
            input_state,
            _subscription,
        };
        panel.load_notes(cx);
        panel
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

        let note = Record::new_note(text);
        eprintln!("[NotePanel] Created note with id: {}", note.id);

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
                        eprintln!("[NotePanel] View update succeeded");
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

    // Parse note content to extract title (first line) and preview (second line)
    fn parse_note_content(content: &str) -> (String, String) {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return ("无标题".to_string(), String::new());
        }

        let title = lines[0].trim().to_string();
        let title = if title.is_empty() {
            "无标题".to_string()
        } else {
            title
        };

        // Get the second non-empty line as preview (if any)
        let preview = lines[1..]
            .iter()
            .find(|line| !line.trim().is_empty())
            .map(|line| line.trim().to_string())
            .unwrap_or_default();

        (title, preview)
    }
}

impl Render for NotePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        eprintln!("[NotePanel] Rendering {} notes", self.notes.len());

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("笔记 ({})", self.notes.len()))
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        Input::new(&self.input_state)
                            .flex_1()
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
                    .child("输入笔记内容，第一行自动作为标题，Enter 保存")
            )
            .child(
                div()
                    .id("note-list")
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .overflow_y_scroll()
                    .children(self.notes.clone().into_iter().enumerate().map(|(idx, note)| {
                        let note_id = note.id;
                        let (title, preview) = Self::parse_note_content(&note.content);

                        div()
                            .id(idx)
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .px(px(12.0))
                            .py(px(8.0))
                            .rounded(px(6.0))
                            .bg(rgb(0xe8e8e8))
                            .text_color(rgb(0x000000))
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
                                            .cursor_pointer()
                                            .text_color(rgb(0x888888))
                                            .child("×")
                                            .id(("delete", idx))
                                            .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                                                this.delete_note(note_id, cx);
                                            }))
                                    )
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0x444444))
                                    .child(if preview.is_empty() { "...".to_string() } else { preview })
                            )
                    }))
            )
    }
}
