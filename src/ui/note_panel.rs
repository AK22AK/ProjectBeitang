use crate::models::Record;
use crate::store::Store;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::button::Button;
use uuid::Uuid;

pub struct NotePanel {
    store: Store,
    notes: Vec<Record>,
    title_input: Entity<InputState>,
    content_input: Entity<InputState>,
    _title_subscription: Subscription,
    _content_subscription: Subscription,
}

impl NotePanel {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("笔记标题")
        });

        let content_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("笔记内容...")
        });

        let _title_subscription = cx.subscribe_in(
            &title_input,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => {
                        // Move focus to content input when Enter is pressed in title
                        this.content_input.focus_handle(cx).focus(window, cx);
                    }
                    _ => {}
                }
            },
        );

        let _content_subscription = cx.subscribe_in(
            &content_input,
            window,
            |_this, _state, _event: &InputEvent, _window, _cx| {
                // Check if Shift is pressed for newline, otherwise create note
                // For now, we'll use a button to create notes
            },
        );

        let mut panel = Self {
            store,
            notes: Vec::new(),
            title_input,
            content_input,
            _title_subscription,
            _content_subscription,
        };
        panel.load_notes(cx);
        panel
    }

    fn load_notes(&mut self, cx: &mut Context<Self>) {
        eprintln!("[NotePanel] load_notes called");
        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            // Retry mechanism: database may not be initialized yet
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
                cx.notify();  // Trigger UI refresh
                eprintln!("[NotePanel] Notes updated and notified, panel now has {} notes", panel.notes.len());
            });
            if let Err(e) = update_result {
                eprintln!("[NotePanel] Failed to update view: {:?}", e);
            }
        }).detach();
    }

    fn create_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.title_input.read(cx).text().to_string();
        let content = self.content_input.read(cx).text().to_string();

        eprintln!("[NotePanel] create_note called with title: '{}', content: '{}'", title, content);

        if title.trim().is_empty() && content.trim().is_empty() {
            eprintln!("[NotePanel] Both title and content are empty, returning");
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
        eprintln!("[NotePanel] Created note with id: {}", note.id);

        // Clear inputs
        self.title_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        self.content_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });

        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            eprintln!("[NotePanel] Spawning create_record...");
            match store.create_record(note).await {
                Ok(_) => {
                    eprintln!("[NotePanel] create_record succeeded, scheduling load_notes");
                    // Delay a bit to ensure database write completes
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

    // Parse note content to extract title (first line) and body (rest)
    fn parse_note_content(content: &str) -> (String, String) {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return ("无标题".to_string(), String::new());
        }

        let title = lines[0].trim().to_string();
        let body = lines[1..]
            .iter()
            .skip_while(|line| line.trim().is_empty())
            .map(|line| *line)
            .collect::<Vec<_>>()
            .join("\n");

        (title, body)
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
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        Input::new(&self.title_input)
                            .w(px(300.0))
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(
                                Input::new(&self.content_input)
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
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x888888))
                    .child("输入标题和内容，点击添加按钮创建笔记")
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
                        let (title, body) = Self::parse_note_content(&note.content);

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
                                    .child(body)
                            )
                    }))
            )
    }
}
