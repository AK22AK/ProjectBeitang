# 全局快捷键与浮动窗口实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 实现系统级全局快捷键（Cmd+N/M/1/2/0），浮动窗口快速添加，主窗口查看管理

**Architecture:** 使用 `global-hotkey` crate 注册系统级快捷键，GPUI 管理多窗口（浮动窗口 + 主窗口），所有窗口共享 Store 数据层

**Tech Stack:** Rust, GPUI, global-hotkey, SQLite

---

## 前置知识

**当前代码结构：**
- `src/main.rs` - 应用入口，创建主窗口
- `src/store.rs` - 异步数据存储层
- `src/db.rs` - SQLite 数据库操作
- `src/models.rs` - 数据模型（Record, Priority）
- `src/ui/task_panel.rs` - 任务面板 UI
- `src/ui/sidebar.rs` - 侧边栏导航
- `tests/` - 集成测试

**关键概念：**
- GPUI 的 `Window` 和 `cx`（Context）用于 UI 渲染
- Store 使用 async-channel 进行异步通信
- 已有测试体系使用 `cargo test` 运行

---

## Task 1: 添加 global-hotkey 依赖

**Files:**
- Modify: `Cargo.toml`

**Step 1: 添加依赖**

```toml
[dependencies]
# ... 现有依赖 ...
global-hotkey = "0.4"
```

**Step 2: 编译验证**

Run: `cargo check`
Expected: 成功，无错误

**Step 3: 提交**

```bash
git add Cargo.toml
git commit -m "deps: add global-hotkey for system-wide shortcuts"
```

---

## Task 2: 创建快捷键配置模块

**Files:**
- Create: `src/config.rs`
- Modify: `src/lib.rs` (添加模块声明)

**Step 1: 编写快捷键配置结构**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutConfig {
    pub quick_add_task: String,
    pub quick_add_note: String,
    pub view_tasks: String,
    pub view_notes: String,
    pub open_main: String,
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        Self {
            quick_add_task: "Cmd+N".to_string(),
            quick_add_note: "Cmd+M".to_string(),
            view_tasks: "Cmd+1".to_string(),
            view_notes: "Cmd+2".to_string(),
            open_main: "Cmd+0".to_string(),
        }
    }
}

impl ShortcutConfig {
    pub fn load() -> Self {
        // 暂时返回默认配置，后续可从文件加载
        Self::default()
    }
}
```

**Step 2: 添加到 lib.rs**

```rust
pub mod config;
// ... 其他模块
```

**Step 3: 编译验证**

Run: `cargo check`
Expected: 成功

**Step 4: 提交**

```bash
git add src/config.rs src/lib.rs
git commit -m "feat: add shortcut configuration module"
```

---

## Task 3: 创建浮动窗口组件

**Files:**
- Create: `src/ui/floating_window.rs`
- Modify: `src/ui/mod.rs`

**Step 1: 创建浮动窗口模块**

```rust
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use crate::models::{Priority, Record};
use crate::store::Store;

pub struct QuickAddWindow {
    store: Store,
    input_state: Entity<InputState>,
    _subscription: Subscription,
}

impl QuickAddWindow {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入任务内容 (Enter 保存, Esc 取消)")
        });

        let _subscription = cx.subscribe_in(
            &input_state,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => {
                        this.submit(window, cx);
                    }
                    _ => {}
                }
            },
        );

        Self {
            store,
            input_state,
            _subscription,
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input_state.read(cx).text().to_string();
        if text.trim().is_empty() {
            return;
        }

        let (content, priority) = parse_quick_input(&text);
        let task = Record::new_task(content, priority);

        let store = self.store.clone();
        cx.spawn(async move |_view, _cx| {
            if let Err(e) = store.create_record(task).await {
                eprintln!("[QuickAdd] Failed to create task: {}", e);
            }
        }).detach();

        // 关闭窗口
        cx.emit(DismissEvent);
    }
}

impl Render for QuickAddWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p(px(16.0))
            .child(Input::new(&self.input_state))
    }
}

// 简化的优先级解析
fn parse_quick_input(input: &str) -> (String, Priority) {
    let trimmed = input.trim();
    if trimmed.starts_with("!!") {
        (trimmed[2..].trim_start().to_string(), Priority::High)
    } else if trimmed.starts_with("!") {
        (trimmed[1..].trim_start().to_string(), Priority::Medium)
    } else {
        (trimmed.to_string(), Priority::Low)
    }
}
```

**Step 2: 添加到 ui/mod.rs**

```rust
pub mod floating_window;
```

**Step 3: 编译验证**

Run: `cargo check`
Expected: 成功

**Step 4: 提交**

```bash
git add src/ui/floating_window.rs src/ui/mod.rs
git commit -m "feat: add floating window for quick task creation"
```

---

## Task 4: 实现 global-hotkey 管理器

**Files:**
- Create: `src/shortcut_manager.rs`
- Modify: `src/lib.rs`

**Step 1: 创建快捷键管理器**

```rust
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use global_hotkey::hotkey::HotKey;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutEvent {
    QuickAddTask,
    QuickAddNote,
    ViewTasks,
    ViewNotes,
    OpenMain,
}

pub struct ShortcutManager {
    manager: GlobalHotKeyManager,
    event_receiver: broadcast::Receiver<ShortcutEvent>,
}

impl ShortcutManager {
    pub fn new() -> anyhow::Result<(Self, broadcast::Receiver<ShortcutEvent>)> {
        let manager = GlobalHotKeyManager::new()?;
        let (tx, rx) = broadcast::channel(10);

        // 注册快捷键
        let hotkeys = vec![
            (HotKey::new(None, global_hotkey::KeyCode::KeyN), ShortcutEvent::QuickAddTask),
            (HotKey::new(None, global_hotkey::KeyCode::KeyM), ShortcutEvent::QuickAddNote),
            // 数字键需要 Cmd 修饰
            (HotKey::new(Some(global_hotkey::Modifiers::COMMAND), global_hotkey::KeyCode::Digit1), ShortcutEvent::ViewTasks),
            (HotKey::new(Some(global_hotkey::Modifiers::COMMAND), global_hotkey::KeyCode::Digit2), ShortcutEvent::ViewNotes),
            (HotKey::new(Some(global_hotkey::Modifiers::COMMAND), global_hotkey::KeyCode::Digit0), ShortcutEvent::OpenMain),
        ];

        for (hotkey, event) in hotkeys {
            manager.register(hotkey)?;
        }

        // 启动监听线程
        std::thread::spawn(move || {
            loop {
                if let Ok(event) = GlobalHotKeyEvent::receiver().recv() {
                    if event.state == HotKeyState::Pressed {
                        // 映射到 ShortcutEvent 并发送
                        // ... 映射逻辑
                    }
                }
            }
        });

        Ok((Self { manager, event_receiver: rx.clone() }, rx))
    }
}
```

**注意：** 这里的代码是示意，需要根据 global-hotkey 实际 API 调整

**Step 2: 添加到 lib.rs**

```rust
pub mod shortcut_manager;
```

**Step 3: 编译验证并修复**

Run: `cargo check`
Expected: 可能需要根据 global-hotkey 实际 API 调整

**Step 4: 提交**

```bash
git add src/shortcut_manager.rs src/lib.rs
git commit -m "feat: add shortcut manager with global-hotkey integration"
```

---

## Task 5: 重构 main.rs 支持多窗口

**Files:**
- Modify: `src/main.rs`

**Step 1: 添加浮动窗口管理**

需要重构 main.rs 来：
1. 初始化 ShortcutManager
2. 监听快捷键事件
3. 根据事件类型显示不同窗口

```rust
use robinne::shortcut_manager::{ShortcutManager, ShortcutEvent};
use robinne::ui::floating_window::QuickAddWindow;
// ... 其他导入

fn main() {
    let app = application();

    app.run(move |cx| {
        gpui_component::init(cx);
        gpui_component::Theme::change(gpui_component::ThemeMode::Light, None, cx);

        let (store, mut runtime) = create_store();

        // 启动 store runtime
        cx.spawn(|_cx: &mut AsyncApp| async move {
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("robinne");
            std::fs::create_dir_all(&data_dir).ok();
            let db_path = data_dir.join("data.db");
            runtime.run(db_path).await;
        }).detach();

        // 初始化快捷键管理器
        let (_shortcut_manager, mut event_rx) = ShortcutManager::new().expect("Failed to create shortcut manager");

        // 监听快捷键事件
        cx.spawn(async move |cx| {
            while let Ok(event) = event_rx.recv().await {
                match event {
                    ShortcutEvent::QuickAddTask => {
                        // 显示浮动窗口
                        cx.update_global(|cx| {
                            // 打开浮动窗口
                        }).ok();
                    }
                    ShortcutEvent::ViewTasks => {
                        // 激活主窗口，切换到任务面板
                    }
                    // ... 其他事件
                }
            }
        }).detach();

        // 打开主窗口
        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|cx| MainView::new(store, window, cx));
                cx.new(|cx| {
                    gpui_component::Root::new(view, window, cx)
                        .bg(cx.theme().background)
                })
            })?;
            Ok::<_, anyhow::Error>(())
        }).detach();
    });
}
```

**Step 2: 编译并修复错误**

Run: `cargo check`
Expected: 根据实际错误调整代码

**Step 3: 运行测试**

Run: `cargo test`
Expected: 所有测试通过

**Step 4: 提交**

```bash
git add src/main.rs
git commit -m "feat: integrate shortcut manager with main application"
```

---

## Task 6: 实现浮动窗口显示

**Files:**
- Modify: `src/main.rs`
- Modify: `src/ui/floating_window.rs`

**Step 1: 配置浮动窗口选项**

```rust
let window_options = WindowOptions {
    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
        None,
        size(px(400.0), px(120.0)),
        cx,
    ))),
    // 其他窗口选项...
    ..Default::default()
};
```

**Step 2: 在快捷键事件中打开浮动窗口**

```rust
ShortcutEvent::QuickAddTask => {
    cx.open_window(window_options, |window, cx| {
        cx.new(|cx| QuickAddWindow::new(store.clone(), window, cx))
    }).ok();
}
```

**Step 3: 测试浮动窗口**

1. 运行应用
2. 按 Cmd+N
3. 验证浮动窗口显示
4. 输入任务，回车
5. 验证任务保存到数据库

**Step 4: 提交**

```bash
git add src/main.rs src/ui/floating_window.rs
git commit -m "feat: implement floating window for quick task creation"
```

---

## Task 7: 实现主窗口激活与面板切换

**Files:**
- Modify: `src/main.rs`
- Modify: `src/ui/sidebar.rs` (添加面板切换支持)

**Step 1: 存储主窗口句柄**

```rust
struct AppState {
    main_window: Option<WindowHandle>,
    current_panel: Panel,
}
```

**Step 2: 实现主窗口激活**

```rust
ShortcutEvent::ViewTasks => {
    if let Some(window) = app_state.main_window {
        // 激活窗口
        window.activate();
        // 切换到任务面板
        app_state.current_panel = Panel::Tasks;
    } else {
        // 创建主窗口
    }
}
```

**Step 3: 提交**

```bash
git add src/main.rs src/ui/sidebar.rs
git commit -m "feat: implement main window activation and panel switching"
```

---

## Task 8: 实现笔记功能

**Files:**
- Create: `src/ui/note_panel.rs`
- Modify: `src/models.rs` (添加笔记类型)
- Modify: `src/db.rs` (支持笔记存储)
- Modify: `src/ui/sidebar.rs` (添加笔记入口)

**Step 1: 扩展 Record 模型支持笔记**

```rust
// models.rs 已支持 RecordType::Note
```

**Step 2: 创建笔记面板**

```rust
pub struct NotePanel {
    store: Store,
    notes: Vec<Record>,
    // ...
}
```

**Step 3: 实现笔记 CRUD**

类似于 task_panel，但支持多行文本

**Step 4: 提交**

```bash
git add src/ui/note_panel.rs src/ui/sidebar.rs
git commit -m "feat: add note panel with CRUD operations"
```

---

## Task 9: 实现 Esc 关闭窗口

**Files:**
- Modify: `src/ui/floating_window.rs`
- Modify: `src/main.rs` (主窗口关闭处理)

**Step 1: 添加键盘事件监听**

```rust
impl QuickAddWindow {
    fn handle_key_event(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            cx.emit(DismissEvent);
        }
    }
}
```

**Step 2: 主窗口 Esc 处理**

```rust
// 在主窗口处理 Esc 键关闭
```

**Step 3: 提交**

```bash
git add src/ui/floating_window.rs src/main.rs
git commit -m "feat: implement Esc key to close windows"
```

---

## Task 10: 添加快捷键相关测试

**Files:**
- Create: `tests/shortcut_tests.rs`

**Step 1: 测试配置加载**

```rust
#[test]
fn test_default_shortcut_config() {
    let config = ShortcutConfig::default();
    assert_eq!(config.quick_add_task, "Cmd+N");
    assert_eq!(config.view_tasks, "Cmd+1");
}
```

**Step 2: 测试优先级解析**

```rust
#[test]
fn test_parse_quick_input() {
    let (content, priority) = parse_quick_input("!!High task");
    assert_eq!(content, "High task");
    assert_eq!(priority, Priority::High);
}
```

**Step 3: 运行所有测试**

Run: `cargo test`
Expected: 所有测试通过

**Step 4: 提交**

```bash
git add tests/shortcut_tests.rs
git commit -m "test: add shortcut and parsing tests"
```

---

## 最终验证

**完整测试清单：**

- [ ] 任意应用下按 Cmd+N 显示浮动窗口
- [ ] 输入任务回车保存并关闭窗口
- [ ] 按 Esc 关闭浮动窗口
- [ ] Cmd+1 激活主窗口并显示任务面板
- [ ] Cmd+2 激活主窗口并显示笔记面板
- [ ] 主窗口按 Esc 关闭
- [ ] 所有数据正确保存到数据库
- [ ] `cargo test` 所有测试通过
- [ ] `cargo build` 无警告

---

## 注意事项

1. **global-hotkey 平台支持** - macOS 需要 accessibility 权限
2. **窗口焦点** - 浮动窗口需要正确处理焦点，避免输入问题
3. **快捷键冲突** - 需要处理与其他应用的快捷键冲突
4. **测试策略** - global-hotkey 的测试需要模拟按键，可能需要特殊处理

---

**计划创建日期:** 2026-03-03
**预计实施时间:** 4-6 小时
**任务数量:** 10 个
