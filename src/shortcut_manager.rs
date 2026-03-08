use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use global_hotkey::hotkey::{HotKey, Code, Modifiers};
use std::sync::mpsc::channel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutEvent {
    QuickAddTask,
    QuickAddNote,
    ViewTasks,
    ViewNotes,
    OpenMain,
}

pub struct ShortcutManager {
    _manager: GlobalHotKeyManager,
}

impl ShortcutManager {
    pub fn new() -> anyhow::Result<(Self, std::sync::mpsc::Receiver<ShortcutEvent>)> {
        let manager = GlobalHotKeyManager::new()?;
        let (tx, rx) = channel::<ShortcutEvent>();

        // 注册快捷键
        // Cmd+N for quick add task
        let hotkey_n = HotKey::new(Some(Modifiers::SUPER), Code::KeyN);
        manager.register(hotkey_n)?;
        eprintln!("[ShortcutManager] Registered Cmd+N with id: {}", hotkey_n.id());

        // Cmd+M for quick add note
        let hotkey_m = HotKey::new(Some(Modifiers::SUPER), Code::KeyM);
        manager.register(hotkey_m)?;
        eprintln!("[ShortcutManager] Registered Cmd+M with id: {}", hotkey_m.id());

        // Cmd+1 for view tasks
        let hotkey_1 = HotKey::new(Some(Modifiers::SUPER), Code::Digit1);
        manager.register(hotkey_1)?;
        eprintln!("[ShortcutManager] Registered Cmd+1 with id: {}", hotkey_1.id());

        // Cmd+2 for view notes
        let hotkey_2 = HotKey::new(Some(Modifiers::SUPER), Code::Digit2);
        manager.register(hotkey_2)?;
        eprintln!("[ShortcutManager] Registered Cmd+2 with id: {}", hotkey_2.id());

        // Cmd+0 for open main
        let hotkey_0 = HotKey::new(Some(Modifiers::SUPER), Code::Digit0);
        manager.register(hotkey_0)?;
        eprintln!("[ShortcutManager] Registered Cmd+0 with id: {}", hotkey_0.id());

        // 启动监听线程
        let tx_clone = tx.clone();
        std::thread::spawn(move || {
            let event_receiver = GlobalHotKeyEvent::receiver();
            loop {
                if let Ok(event) = event_receiver.recv() {
                    if event.state == HotKeyState::Pressed {
                        let shortcut_event = match event.id {
                            id if id == hotkey_n.id() => Some(ShortcutEvent::QuickAddTask),
                            id if id == hotkey_m.id() => Some(ShortcutEvent::QuickAddNote),
                            id if id == hotkey_1.id() => Some(ShortcutEvent::ViewTasks),
                            id if id == hotkey_2.id() => Some(ShortcutEvent::ViewNotes),
                            id if id == hotkey_0.id() => Some(ShortcutEvent::OpenMain),
                            _ => None,
                        };

                        if let Some(se) = shortcut_event {
                            let _ = tx_clone.send(se);
                        }
                    }
                }
            }
        });

        Ok((Self { _manager: manager }, rx))
    }
}
