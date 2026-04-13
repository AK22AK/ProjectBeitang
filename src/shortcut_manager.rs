use crate::config::ShortcutConfig;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::mpsc::channel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutEvent {
    QuickCapture,
    OpenMain,
    OpenTasks,
    OpenRecords,
}

pub struct ShortcutManager {
    _manager: GlobalHotKeyManager,
}

impl ShortcutManager {
    pub fn new() -> anyhow::Result<(Self, std::sync::mpsc::Receiver<ShortcutEvent>)> {
        let manager = GlobalHotKeyManager::new()?;
        let config = ShortcutConfig::load();
        let (tx, rx) = channel::<ShortcutEvent>();

        let quick_capture = register_hotkey(
            &manager,
            config.quick_capture_hotkey()?,
            &config.quick_capture,
        )?;

        let tx_clone = tx.clone();
        std::thread::spawn(move || {
            let event_receiver = GlobalHotKeyEvent::receiver();
            loop {
                if let Ok(event) = event_receiver.recv() {
                    if event.state == HotKeyState::Released {
                        let shortcut_event = match event.id {
                            id if id == quick_capture.id() => Some(ShortcutEvent::QuickCapture),
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

fn register_hotkey(
    manager: &GlobalHotKeyManager,
    hotkey: HotKey,
    label: &str,
) -> anyhow::Result<HotKey> {
    manager.register(hotkey)?;
    eprintln!(
        "[ShortcutManager] Registered {} with id: {}",
        label,
        hotkey.id()
    );
    Ok(hotkey)
}
