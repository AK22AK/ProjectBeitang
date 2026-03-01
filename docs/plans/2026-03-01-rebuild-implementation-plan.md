# Project Beitang 重建实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 从零构建基于 GPUI + SQLite 的闪念大脑应用，支持任务/记录/时间线/AI 四个面板和全局快捷键。

**Architecture:** 异步架构，UI 层通过通道与后台 SQLite 操作解耦。GPUI 负责 GPU 加速渲染，rusqlite 处理持久化，数据模型支持任务/想法/事件三种类型。

**Tech Stack:** GPUI, gpui-component, rusqlite, tokio/async-channel, serde

---

## Phase 1: 项目初始化和基础依赖

### Task 1: 创建 Rust 项目结构

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `.gitignore`

**Step 1: 初始化 Cargo 项目**

Run: `cargo init --name beitang`

**Step 2: 编辑 Cargo.toml 添加依赖**

```toml
[package]
name = "beitang"
version = "0.1.0"
edition = "2021"

[dependencies]
# GPUI 框架
gpui = { git = "https://github.com/zed-industries/zed" }
gpui-component = { git = "https://github.com/yourusername/gpui-component" }

# 异步运行时
tokio = { version = "1.0", features = ["rt-multi-thread", "macros", "sync"] }
async-channel = "2.0"

# 数据库
rusqlite = { version = "0.30", features = ["bundled", "chrono"] }

# 序列化和配置
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# 工具
uuid = { version = "1.0", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1.0"
thiserror = "1.0"

[dev-dependencies]
tempfile = "3.0"
```

**Step 3: 创建基础 main.rs**

```rust
fn main() {
    println!("Hello, Beitang!");
}
```

**Step 4: 创建 .gitignore**

```
/target
*.db
*.db-journal
.DS_Store
```

**Step 5: 验证编译**

Run: `cargo check`
Expected: Compiles without errors

**Step 6: Commit**

```bash
git add Cargo.toml src/main.rs .gitignore
git commit -m "chore: initialize project with dependencies"
```

---

## Phase 2: 数据库层和异步架构

### Task 2: 定义数据模型

**Files:**
- Create: `src/models.rs`

**Step 1: 编写数据模型**

```rust
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordType {
    Task,
    Idea,
    Event,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub id: Uuid,
    pub content: String,
    pub created_at: DateTime<Local>,
    pub record_type: RecordType,
    pub priority: Option<Priority>,
    pub started_at: Option<DateTime<Local>>,
    pub completed_at: Option<DateTime<Local>>,
    pub source_record_id: Option<Uuid>,
}

impl Record {
    pub fn new_task(content: impl Into<String>, priority: Priority) -> Self {
        let now = Local::now();
        Self {
            id: Uuid::new_v4(),
            content: content.into(),
            created_at: now,
            record_type: RecordType::Task,
            priority: Some(priority),
            started_at: Some(now),
            completed_at: None,
            source_record_id: None,
        }
    }

    pub fn new_idea(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            content: content.into(),
            created_at: Local::now(),
            record_type: RecordType::Idea,
            priority: None,
            started_at: None,
            completed_at: None,
            source_record_id: None,
        }
    }

    pub fn new_event(content: impl Into<String>) -> Self {
        let now = Local::now();
        Self {
            id: Uuid::new_v4(),
            content: content.into(),
            created_at: now,
            record_type: RecordType::Event,
            priority: None,
            started_at: Some(now),
            completed_at: Some(now),
            source_record_id: None,
        }
    }

    pub fn is_completed(&self) -> bool {
        self.completed_at.is_some()
    }

    pub fn complete(&mut self) {
        if self.record_type == RecordType::Task && !self.is_completed() {
            self.completed_at = Some(Local::now());
        }
    }
}
```

**Step 2: 添加到 main.rs 模块**

```rust
mod models;
```

**Step 3: 验证编译**

Run: `cargo check`
Expected: Compiles without errors

**Step 4: Commit**

```bash
git add src/models.rs src/main.rs
git commit -m "feat: add Record data model with Task/Idea/Event types"
```

---

### Task 3: 创建数据库管理器

**Files:**
- Create: `src/db.rs`
- Create: `tests/db_test.rs`

**Step 1: 编写数据库管理器**

```rust
use crate::models::{Priority, Record, RecordType};
use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use rusqlite::{params, Connection};
use std::path::Path;
use uuid::Uuid;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init_tables()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.init_tables()?;
        Ok(db)
    }

    fn init_tables(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS records (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                record_type TEXT NOT NULL,
                priority TEXT,
                started_at TEXT,
                completed_at TEXT,
                source_record_id TEXT
            )",
            [],
        )?;

        // Create index for efficient querying
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_records_type ON records(record_type)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_records_created ON records(created_at)",
            [],
        )?;

        Ok(())
    }

    pub fn insert_record(&self, record: &Record) -> Result<()> {
        self.conn.execute(
            "INSERT INTO records (id, content, created_at, record_type, priority, started_at, completed_at, source_record_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                record.id.to_string(),
                record.content,
                record.created_at.to_rfc3339(),
                serde_json::to_string(&record.record_type)?,
                record.priority.as_ref().map(|p| serde_json::to_string(p).unwrap()),
                record.started_at.map(|d| d.to_rfc3339()),
                record.completed_at.map(|d| d.to_rfc3339()),
                record.source_record_id.map(|id| id.to_string()),
            ],
        )?;
        Ok(())
    }

    pub fn get_record(&self, id: Uuid) -> Result<Option<Record>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, content, created_at, record_type, priority, started_at, completed_at, source_record_id
             FROM records WHERE id = ?1"
        )?;

        let record = stmt.query_row([id.to_string()], |row| {
            Self::row_to_record(row)
        }).optional()?;

        Ok(record)
    }

    pub fn get_all_records(&self) -> Result<Vec<Record>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, content, created_at, record_type, priority, started_at, completed_at, source_record_id
             FROM records ORDER BY created_at DESC"
        )?;

        let records = stmt.query_map([], |row| {
            Self::row_to_record(row)
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(records)
    }

    pub fn get_tasks(&self, include_completed: bool) -> Result<Vec<Record>> {
        let sql = if include_completed {
            "SELECT * FROM records WHERE record_type = '\"Task\"' ORDER BY created_at DESC"
        } else {
            "SELECT * FROM records WHERE record_type = '\"Task\"' AND completed_at IS NULL ORDER BY created_at DESC"
        };

        let mut stmt = self.conn.prepare(sql)?;
        let records = stmt.query_map([], |row| {
            Self::row_to_record(row)
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(records)
    }

    pub fn update_record(&self, record: &Record) -> Result<()> {
        self.conn.execute(
            "UPDATE records SET
                content = ?2,
                record_type = ?3,
                priority = ?4,
                started_at = ?5,
                completed_at = ?6,
                source_record_id = ?7
             WHERE id = ?1",
            params![
                record.id.to_string(),
                record.content,
                serde_json::to_string(&record.record_type)?,
                record.priority.as_ref().map(|p| serde_json::to_string(p).unwrap()),
                record.started_at.map(|d| d.to_rfc3339()),
                record.completed_at.map(|d| d.to_rfc3339()),
                record.source_record_id.map(|id| id.to_string()),
            ],
        )?;
        Ok(())
    }

    pub fn delete_record(&self, id: Uuid) -> Result<()> {
        self.conn.execute(
            "DELETE FROM records WHERE id = ?1",
            [id.to_string()],
        )?;
        Ok(())
    }

    fn row_to_record(row: &rusqlite::Row) -> rusqlite::Result<Record> {
        let id: String = row.get(0)?;
        let content: String = row.get(1)?;
        let created_at: String = row.get(2)?;
        let record_type: String = row.get(3)?;
        let priority: Option<String> = row.get(4)?;
        let started_at: Option<String> = row.get(5)?;
        let completed_at: Option<String> = row.get(6)?;
        let source_record_id: Option<String> = row.get(7)?;

        Ok(Record {
            id: Uuid::parse_str(&id).map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                0, rusqlite::types::Type::Text, Box::new(e)
            ))?,
            content,
            created_at: DateTime::parse_from_rfc3339(&created_at)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                    2, rusqlite::types::Type::Text, Box::new(e)
                ))?.with_timezone(&Local),
            record_type: serde_json::from_str(&record_type)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                    3, rusqlite::types::Type::Text, Box::new(e)
                ))?,
            priority: priority.map(|p| serde_json::from_str(&p).unwrap()),
            started_at: started_at.map(|d| DateTime::parse_from_rfc3339(&d).unwrap().with_timezone(&Local)),
            completed_at: completed_at.map(|d| DateTime::parse_from_rfc3339(&d).unwrap().with_timezone(&Local)),
            source_record_id: source_record_id.map(|id| Uuid::parse_str(&id).unwrap()),
        })
    }
}
```

**Step 2: 编写测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Priority, RecordType};

    #[test]
    fn test_create_and_get_record() {
        let db = Database::open_in_memory().unwrap();
        let record = Record::new_task("Test task", Priority::High);

        db.insert_record(&record).unwrap();
        let retrieved = db.get_record(record.id).unwrap().unwrap();

        assert_eq!(retrieved.content, "Test task");
        assert_eq!(retrieved.record_type, RecordType::Task);
        assert_eq!(retrieved.priority, Some(Priority::High));
    }

    #[test]
    fn test_get_tasks_filters_by_type() {
        let db = Database::open_in_memory().unwrap();

        let task = Record::new_task("A task", Priority::Medium);
        let idea = Record::new_idea("An idea");

        db.insert_record(&task).unwrap();
        db.insert_record(&idea).unwrap();

        let tasks = db.get_tasks(true).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].content, "A task");
    }

    #[test]
    fn test_complete_task() {
        let db = Database::open_in_memory().unwrap();
        let mut task = Record::new_task("Task to complete", Priority::Low);

        db.insert_record(&task).unwrap();
        assert!(!task.is_completed());

        task.complete();
        db.update_record(&task).unwrap();

        let retrieved = db.get_record(task.id).unwrap().unwrap();
        assert!(retrieved.is_completed());
    }
}
```

**Step 3: 添加到 main.rs**

```rust
mod db;
```

**Step 4: 运行测试**

Run: `cargo test`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src/db.rs src/main.rs
mkdir -p tests
git add tests/db_test.rs
git commit -m "feat: add SQLite database layer with CRUD operations"
```

---

### Task 4: 创建异步通道架构

**Files:**
- Create: `src/store.rs`

**Step 1: 定义 Store 命令和响应**

```rust
use crate::models::Record;
use anyhow::Result;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum StoreCommand {
    CreateRecord(Record),
    GetRecord(Uuid),
    GetAllRecords,
    GetTasks { include_completed: bool },
    UpdateRecord(Record),
    DeleteRecord(Uuid),
}

#[derive(Debug, Clone)]
pub enum StoreResponse {
    RecordCreated(Uuid),
    Record(Option<Record>),
    Records(Vec<Record>),
    RecordUpdated,
    RecordDeleted,
    Error(String),
}

pub type StoreSender = async_channel::Sender<(StoreCommand, async_channel::Sender<StoreResponse>)>;
pub type StoreReceiver = async_channel::Receiver<(StoreCommand, async_channel::Sender<StoreResponse>)>;
```

**Step 2: 创建 Store 运行时**

```rust
use crate::db::Database;
use std::path::Path;

pub struct StoreRuntime {
    receiver: StoreReceiver,
}

impl StoreRuntime {
    pub fn new(receiver: StoreReceiver) -> Self {
        Self { receiver }
    }

    pub async fn run<P: AsRef<Path>>(self, db_path: P) -> Result<()> {
        let db = Database::open(db_path)?;

        while let Ok((command, responder)) = self.receiver.recv().await {
            let response = Self::handle_command(&db, command);
            let _ = responder.send(response).await;
        }

        Ok(())
    }

    fn handle_command(db: &Database, command: StoreCommand) -> StoreResponse {
        match command {
            StoreCommand::CreateRecord(record) => {
                let id = record.id;
                match db.insert_record(&record) {
                    Ok(_) => StoreResponse::RecordCreated(id),
                    Err(e) => StoreResponse::Error(e.to_string()),
                }
            }
            StoreCommand::GetRecord(id) => {
                match db.get_record(id) {
                    Ok(record) => StoreResponse::Record(record),
                    Err(e) => StoreResponse::Error(e.to_string()),
                }
            }
            StoreCommand::GetAllRecords => {
                match db.get_all_records() {
                    Ok(records) => StoreResponse::Records(records),
                    Err(e) => StoreResponse::Error(e.to_string()),
                }
            }
            StoreCommand::GetTasks { include_completed } => {
                match db.get_tasks(include_completed) {
                    Ok(records) => StoreResponse::Records(records),
                    Err(e) => StoreResponse::Error(e.to_string()),
                }
            }
            StoreCommand::UpdateRecord(record) => {
                match db.update_record(&record) {
                    Ok(_) => StoreResponse::RecordUpdated,
                    Err(e) => StoreResponse::Error(e.to_string()),
                }
            }
            StoreCommand::DeleteRecord(id) => {
                match db.delete_record(id) {
                    Ok(_) => StoreResponse::RecordDeleted,
                    Err(e) => StoreResponse::Error(e.to_string()),
                }
            }
        }
    }
}
```

**Step 3: 创建 Store 客户端**

```rust
#[derive(Clone)]
pub struct Store {
    sender: StoreSender,
}

impl Store {
    pub fn new(sender: StoreSender) -> Self {
        Self { sender }
    }

    pub async fn create_record(&self, record: Record) -> Result<Uuid> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender.send((StoreCommand::CreateRecord(record), tx)).await?;

        match rx.recv().await? {
            StoreResponse::RecordCreated(id) => Ok(id),
            StoreResponse::Error(e) => Err(anyhow::anyhow!(e)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn get_record(&self, id: Uuid) -> Result<Option<Record>> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender.send((StoreCommand::GetRecord(id), tx)).await?;

        match rx.recv().await? {
            StoreResponse::Record(record) => Ok(record),
            StoreResponse::Error(e) => Err(anyhow::anyhow!(e)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn get_all_records(&self) -> Result<Vec<Record>> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender.send((StoreCommand::GetAllRecords, tx)).await?;

        match rx.recv().await? {
            StoreResponse::Records(records) => Ok(records),
            StoreResponse::Error(e) => Err(anyhow::anyhow!(e)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn get_tasks(&self, include_completed: bool) -> Result<Vec<Record>> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender.send((StoreCommand::GetTasks { include_completed }, tx)).await?;

        match rx.recv().await? {
            StoreResponse::Records(records) => Ok(records),
            StoreResponse::Error(e) => Err(anyhow::anyhow!(e)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn update_record(&self, record: Record) -> Result<()> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender.send((StoreCommand::UpdateRecord(record), tx)).await?;

        match rx.recv().await? {
            StoreResponse::RecordUpdated => Ok(()),
            StoreResponse::Error(e) => Err(anyhow::anyhow!(e)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn delete_record(&self, id: Uuid) -> Result<()> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender.send((StoreCommand::DeleteRecord(id), tx)).await?;

        match rx.recv().await? {
            StoreResponse::RecordDeleted => Ok(()),
            StoreResponse::Error(e) => Err(anyhow::anyhow!(e)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }
}

pub fn create_store() -> (Store, StoreRuntime) {
    let (sender, receiver) = async_channel::unbounded();
    let store = Store::new(sender);
    let runtime = StoreRuntime::new(receiver);
    (store, runtime)
}
```

**Step 4: 添加到 main.rs**

```rust
mod store;
```

**Step 5: 验证编译**

Run: `cargo check`
Expected: Compiles without errors

**Step 6: Commit**

```bash
git add src/store.rs src/main.rs
git commit -m "feat: add async store with channel-based architecture"
```

---

## Phase 3: GPUI 基础框架

### Task 5: 创建 GPUI 应用入口

**Files:**
- Modify: `src/main.rs`

**Step 1: 重写 main.rs 为 GPUI 应用**

```rust
mod models;
mod db;
mod store;

use gpui::*;
use store::{create_store, Store, StoreRuntime};
use std::sync::Arc;

fn main() {
    App::new().run(|cx: &mut AppContext| {
        // Create async store
        let (store, runtime) = create_store();

        // Spawn store runtime in background
        cx.spawn(|_| async move {
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("beitang");
            std::fs::create_dir_all(&data_dir).ok();

            let db_path = data_dir.join("data.db");
            runtime.run(db_path).await.ok();
        }).detach();

        // Open main window
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1200.0), px(800.0)),
                    cx,
                ))),
                ..Default::default()
            },
            |cx| {
                cx.new_view(|cx| MainView::new(store, cx))
            },
        ).unwrap();
    });
}

pub struct MainView {
    store: Store,
}

impl MainView {
    pub fn new(store: Store, _cx: &mut ViewContext<Self>) -> Self {
        Self { store }
    }
}

impl Render for MainView {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x1a1a1a))
            .child("Hello, Beitang!")
    }
}
```

**Step 2: 添加 dirs 依赖到 Cargo.toml**

```toml
dirs = "5.0"
```

**Step 3: 验证编译**

Run: `cargo check`
Expected: Compiles without errors (GPUI may take time to compile)

**Step 4: Commit**

```bash
git add Cargo.toml src/main.rs
git commit -m "feat: add GPUI application entry with async store integration"
```

---

### Task 6: 创建侧边栏导航

**Files:**
- Create: `src/ui/sidebar.rs`
- Create: `src/ui/mod.rs`
- Modify: `src/main.rs`

**Step 1: 创建 ui 模块**

```rust
// src/ui/mod.rs
pub mod sidebar;
```

**Step 2: 创建侧边栏组件**

```rust
// src/ui/sidebar.rs
use gpui::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Tasks,
    Records,
    Timeline,
    Ai,
}

pub struct Sidebar {
    current_panel: Panel,
    on_panel_select: Box<dyn Fn(Panel, &mut WindowContext)>,
}

impl Sidebar {
    pub fn new<F>(on_select: F) -> Self
    where
        F: Fn(Panel, &mut WindowContext) + 'static,
    {
        Self {
            current_panel: Panel::Tasks,
            on_panel_select: Box::new(on_select),
        }
    }

    pub fn set_panel(&mut self, panel: Panel) {
        self.current_panel = panel;
    }
}

impl RenderOnce for Sidebar {
    fn render(self, _cx: &mut WindowContext) -> impl IntoElement {
        let items = vec![
            (Panel::Tasks, "📋", "任务"),
            (Panel::Records, "📝", "记录"),
            (Panel::Timeline, "⏰", "时间线"),
            (Panel::Ai, "🤖", "AI"),
        ];

        div()
            .w(px(200.0))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .bg(rgb(0x252525))
            .p(px(12.0))
            .children(items.into_iter().map(|(panel, icon, label)| {
                let is_active = self.current_panel == panel;
                let on_click = self.on_panel_select.clone();

                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .when(is_active, |this| this.bg(rgb(0x3a3a3a)))
                    .hover(|style| style.bg(rgb(0x333333)))
                    .flex()
                    .gap(px(8.0))
                    .items_center()
                    .child(icon)
                    .child(label)
                    .on_click(move |_, cx| on_click(panel, cx))
            }))
    }
}
```

**Step 3: 修改 main.rs 集成侧边栏**

```rust
mod ui;

use ui::sidebar::{Panel, Sidebar};

pub struct MainView {
    store: Store,
    current_panel: Panel,
}

impl MainView {
    pub fn new(store: Store, _cx: &mut ViewContext<Self>) -> Self {
        Self {
            store,
            current_panel: Panel::Tasks,
        }
    }
}

impl Render for MainView {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let current_panel = self.current_panel;
        let on_panel_change = cx.listener(|this, panel: Panel, _cx| {
            this.current_panel = panel;
        });

        div()
            .size_full()
            .flex()
            .bg(rgb(0x1a1a1a))
            .text_color(rgb(0xffffff))
            .child(Sidebar::new(move |panel, _cx| on_panel_change(panel, _cx)).with_panel(current_panel))
            .child(
                div()
                    .flex_1()
                    .p(px(24.0))
                    .child(format!("{:?} Panel", current_panel))
            )
    }
}
```

**Step 4: 修改 Sidebar 支持 with_panel**

```rust
impl Sidebar {
    pub fn with_panel(mut self, panel: Panel) -> Self {
        self.current_panel = panel;
        self
    }
    // ...
}
```

**Step 5: 验证编译和运行**

Run: `cargo run`
Expected: Window opens with sidebar showing 4 panels

**Step 6: Commit**

```bash
git add src/ui/ src/main.rs
git commit -m "feat: add sidebar navigation with 4 panels"
```

---

## Phase 4: 任务面板

### Task 7: 创建任务列表视图

**Files:**
- Create: `src/ui/task_panel.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/ui/sidebar.rs`
- Modify: `src/main.rs`

**Step 1: 创建任务面板**

```rust
// src/ui/task_panel.rs
use crate::models::{Priority, Record};
use crate::store::Store;
use gpui::*;

pub struct TaskPanel {
    store: Store,
    tasks: Vec<Record>,
    input_value: String,
}

impl TaskPanel {
    pub fn new(store: Store, cx: &mut ViewContext<Self>) -> Self {
        let mut panel = Self {
            store,
            tasks: Vec::new(),
            input_value: String::new(),
        };
        panel.load_tasks(cx);
        panel
    }

    fn load_tasks(&mut self, cx: &mut ViewContext<Self>) {
        let store = self.store.clone();
        cx.spawn(|view, mut cx| async move {
            match store.get_tasks(false).await {
                Ok(tasks) => {
                    view.update(&mut cx, |panel, _cx| {
                        panel.tasks = tasks;
                    }).ok();
                }
                Err(e) => eprintln!("Failed to load tasks: {}", e),
            }
        }).detach();
    }

    fn create_task(&mut self, cx: &mut ViewContext<Self>) {
        if self.input_value.trim().is_empty() {
            return;
        }

        let (content, priority) = self.parse_input(&self.input_value);
        let task = Record::new_task(content, priority);

        let store = self.store.clone();
        cx.spawn(|view, mut cx| async move {
            match store.create_record(task).await {
                Ok(_) => {
                    view.update(&mut cx, |panel, cx| {
                        panel.input_value.clear();
                        panel.load_tasks(cx);
                    }).ok();
                }
                Err(e) => eprintln!("Failed to create task: {}", e),
            }
        }).detach();
    }

    fn parse_input(&self, input: &str) -> (String, Priority) {
        let trimmed = input.trim();
        if trimmed.starts_with("!! ") {
            (trimmed[3..].to_string(), Priority::High)
        } else if trimmed.starts_with("! ") {
            (trimmed[2..].to_string(), Priority::Medium)
        } else {
            (trimmed.to_string(), Priority::Low)
        }
    }

    fn complete_task(&mut self, task_id: uuid::Uuid, cx: &mut ViewContext<Self>) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.complete();
            let updated_task = task.clone();
            let store = self.store.clone();

            cx.spawn(|view, mut cx| async move {
                match store.update_record(updated_task).await {
                    Ok(_) => {
                        view.update(&mut cx, |panel, cx| {
                            panel.load_tasks(cx);
                        }).ok();
                    }
                    Err(e) => eprintln!("Failed to complete task: {}", e),
                }
            }).detach();
        }
    }
}

impl Render for TaskPanel {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let input_value = self.input_value.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("任务")
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        input()
                            .placeholder("!! 高优先级任务 | ! 普通任务 | 直接输入")
                            .value(input_value)
                            .on_change(cx.listener(|this, value: String, _cx| {
                                this.input_value = value;
                            }))
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, cx| {
                                if event.keystroke.key == "enter" {
                                    this.create_task(cx);
                                }
                            }))
                    )
                    .child(
                        button("添加")
                            .on_click(cx.listener(|this, _event: &ClickEvent, cx| {
                                this.create_task(cx);
                            }))
                    )
            )
            .child(
                div()
                    .flex_1()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .children(self.tasks.clone().into_iter().map(|task| {
                        let task_id = task.id;
                        let is_completed = task.is_completed();
                        let priority_emoji = match task.priority {
                            Some(Priority::High) => "🔴",
                            Some(Priority::Medium) => "🟡",
                            Some(Priority::Low) => "🟢",
                            None => "⚪",
                        };

                        div()
                            .flex()
                            .gap(px(8.0))
                            .items_center()
                            .px(px(12.0))
                            .py(px(8.0))
                            .rounded(px(6.0))
                            .bg(rgb(0x252525))
                            .child(
                                checkbox()
                                    .checked(is_completed)
                                    .on_click(cx.listener(move |this, _event: &ClickEvent, cx| {
                                        this.complete_task(task_id, cx);
                                    }))
                            )
                            .child(priority_emoji)
                            .child(task.content)
                    }))
            )
    }
}
```

**Step 2: 修改 ui/mod.rs**

```rust
pub mod sidebar;
pub mod task_panel;
```

**Step 3: 修改 main.rs 集成 TaskPanel**

```rust
use ui::task_panel::TaskPanel;

// In render method:
.child(
    div()
        .flex_1()
        .p(px(24.0))
        .child(match self.current_panel {
            Panel::Tasks => cx.new_view(|cx| TaskPanel::new(self.store.clone(), cx)).into_any_element(),
            _ => div().child(format!("{:?} Panel", self.current_panel)).into_any_element(),
        })
)
```

**Step 4: 修复编译问题**

GPUI API 可能需要调整，根据实际错误修改。

**Step 5: 验证功能**

Run: `cargo run`
Expected:
- 任务面板显示
- 可以输入任务内容
- 按 Enter 或点击添加创建任务
- 任务列表自动刷新

**Step 6: Commit**

```bash
git add src/ui/task_panel.rs src/ui/mod.rs src/main.rs
git commit -m "feat: add task panel with create and complete functionality"
```

---

## Phase 5: 记录面板和时间线面板

### Task 8: 创建记录面板

**Files:**
- Create: `src/ui/record_panel.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/main.rs`

**Step 1: 创建记录面板**

```rust
// src/ui/record_panel.rs
use crate::models::Record;
use crate::store::Store;
use gpui::*;

pub struct RecordPanel {
    store: Store,
    records: Vec<Record>,
    input_value: String,
}

impl RecordPanel {
    pub fn new(store: Store, cx: &mut ViewContext<Self>) -> Self {
        let mut panel = Self {
            store,
            records: Vec::new(),
            input_value: String::new(),
        };
        panel.load_records(cx);
        panel
    }

    fn load_records(&mut self, cx: &mut ViewContext<Self>) {
        let store = self.store.clone();
        cx.spawn(|view, mut cx| async move {
            match store.get_all_records().await {
                Ok(records) => {
                    let ideas_and_events: Vec<_> = records
                        .into_iter()
                        .filter(|r| matches!(r.record_type, crate::models::RecordType::Idea | crate::models::RecordType::Event))
                        .collect();

                    view.update(&mut cx, |panel, _cx| {
                        panel.records = ideas_and_events;
                    }).ok();
                }
                Err(e) => eprintln!("Failed to load records: {}", e),
            }
        }).detach();
    }

    fn create_record(&mut self, cx: &mut ViewContext<Self>) {
        if self.input_value.trim().is_empty() {
            return;
        }

        let record = Record::new_idea(&self.input_value);
        let store = self.store.clone();

        cx.spawn(|view, mut cx| async move {
            match store.create_record(record).await {
                Ok(_) => {
                    view.update(&mut cx, |panel, cx| {
                        panel.input_value.clear();
                        panel.load_records(cx);
                    }).ok();
                }
                Err(e) => eprintln!("Failed to create record: {}", e),
            }
        }).detach();
    }
}

impl Render for RecordPanel {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let input_value = self.input_value.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("记录")
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        input()
                            .placeholder("记录想法或刚才做了什么...")
                            .value(input_value)
                            .on_change(cx.listener(|this, value: String, _cx| {
                                this.input_value = value;
                            }))
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, cx| {
                                if event.keystroke.key == "enter" {
                                    this.create_record(cx);
                                }
                            }))
                    )
                    .child(
                        button("添加")
                            .on_click(cx.listener(|this, _event: &ClickEvent, cx| {
                                this.create_record(cx);
                            }))
                    )
            )
            .child(
                div()
                    .flex_1()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .children(self.records.clone().into_iter().map(|record| {
                        let type_emoji = match record.record_type {
                            crate::models::RecordType::Idea => "💡",
                            crate::models::RecordType::Event => "✓",
                            _ => "•",
                        };

                        div()
                            .px(px(12.0))
                            .py(px(8.0))
                            .rounded(px(6.0))
                            .bg(rgb(0x252525))
                            .child(format!("{} {}", type_emoji, record.content))
                    }))
            )
    }
}
```

**Step 2: 修改 main.rs 集成**

```rust
use ui::record_panel::RecordPanel;

// In render match:
Panel::Records => cx.new_view(|cx| RecordPanel::new(self.store.clone(), cx)).into_any_element(),
```

**Step 3: Commit**

```bash
git add src/ui/record_panel.rs src/ui/mod.rs src/main.rs
git commit -m "feat: add record panel for ideas and events"
```

---

### Task 9: 创建时间线面板

**Files:**
- Create: `src/ui/timeline_panel.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/main.rs`

**Step 1: 创建时间线面板**

```rust
// src/ui/timeline_panel.rs
use crate::models::{Record, RecordType, Priority};
use crate::store::Store;
use chrono::{Local, Datelike};
use gpui::*;
use std::collections::BTreeMap;

pub struct TimelinePanel {
    store: Store,
    grouped_records: BTreeMap<String, Vec<Record>>,
}

impl TimelinePanel {
    pub fn new(store: Store, cx: &mut ViewContext<Self>) -> Self {
        let mut panel = Self {
            store,
            grouped_records: BTreeMap::new(),
        };
        panel.load_records(cx);
        panel
    }

    fn load_records(&mut self, cx: &mut ViewContext<Self>) {
        let store = self.store.clone();
        cx.spawn(|view, mut cx| async move {
            match store.get_all_records().await {
                Ok(records) => {
                    let grouped = Self::group_by_date(records);
                    view.update(&mut cx, |panel, _cx| {
                        panel.grouped_records = grouped;
                    }).ok();
                }
                Err(e) => eprintln!("Failed to load records: {}", e),
            }
        }).detach();
    }

    fn group_by_date(records: Vec<Record>) -> BTreeMap<String, Vec<Record>> {
        let mut grouped: BTreeMap<String, Vec<Record>> = BTreeMap::new();

        for record in records {
            let date_key = Self::format_date_group(&record.created_at);
            grouped.entry(date_key).or_default().push(record);
        }

        grouped
    }

    fn format_date_group(date: &chrono::DateTime<Local>) -> String {
        let today = Local::now().date_naive();
        let record_date = date.date_naive();

        if record_date == today {
            "今天".to_string()
        } else if record_date == today.pred_opt().unwrap() {
            "昨天".to_string()
        } else {
            date.format("%Y年%m月%d日").to_string()
        }
    }

    fn complete_task(&mut self, task_id: uuid::Uuid, cx: &mut ViewContext<Self>) {
        let store = self.store.clone();

        cx.spawn(|view, mut cx| async move {
            if let Ok(Some(mut task)) = store.get_record(task_id).await {
                task.complete();
                match store.update_record(task).await {
                    Ok(_) => {
                        view.update(&mut cx, |panel, cx| {
                            panel.load_records(cx);
                        }).ok();
                    }
                    Err(e) => eprintln!("Failed to complete task: {}", e),
                }
            }
        }).detach();
    }
}

impl Render for TimelinePanel {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("时间线")
            )
            .child(
                div()
                    .flex_1()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .children(self.grouped_records.iter().map(|(date, records)| {
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0x888888))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(date.clone())
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .children(records.iter().map(|record| {
                                        self.render_record_item(record, cx)
                                    }))
                            )
                    }))
            )
    }
}

impl TimelinePanel {
    fn render_record_item(&self, record: &Record, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let task_id = record.id;
        let is_task = matches!(record.record_type, RecordType::Task);
        let is_completed = record.is_completed();

        let (icon, bg_color) = match record.record_type {
            RecordType::Task => {
                let priority_color = match record.priority {
                    Some(Priority::High) => rgb(0x5c3a3a),
                    Some(Priority::Medium) => rgb(0x5c5c3a),
                    Some(Priority::Low) => rgb(0x3a5c3a),
                    None => rgb(0x3a3a3a),
                };
                (if is_completed { "☑" } else { "☐" }, priority_color)
            }
            RecordType::Idea => ("💡", rgb(0x3a3a5c)),
            RecordType::Event => ("✓", rgb(0x3a5c5c)),
        };

        let time_str = record.created_at.format("%H:%M").to_string();

        div()
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(6.0))
            .bg(bg_color)
            .flex()
            .gap(px(8.0))
            .items_center()
            .when(is_task, |this| {
                this.child(
                    div()
                        .cursor_pointer()
                        .child(icon)
                        .on_click(cx.listener(move |this, _event: &ClickEvent, cx| {
                            if !is_completed {
                                this.complete_task(task_id, cx);
                            }
                        }))
                )
            })
            .unless(is_task, |this| this.child(icon))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x888888))
                    .child(time_str)
            )
            .child(record.content.clone())
    }
}
```

**Step 2: 修改 main.rs 集成**

```rust
use ui::timeline_panel::TimelinePanel;

// In render match:
Panel::Timeline => cx.new_view(|cx| TimelinePanel::new(self.store.clone(), cx)).into_any_element(),
```

**Step 3: Commit**

```bash
git add src/ui/timeline_panel.rs src/ui/mod.rs src/main.rs
git commit -m "feat: add timeline panel with date grouping"
```

---

## Phase 6: 搜索和 AI 面板

### Task 10: 创建搜索栏

**Files:**
- Create: `src/ui/search_bar.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/main.rs`

**Step 1: 创建搜索栏组件**

```rust
// src/ui/search_bar.rs
use gpui::*;

pub struct SearchBar {
    value: String,
    on_search: Box<dyn Fn(String, &mut WindowContext)>,
}

impl SearchBar {
    pub fn new<F>(on_search: F) -> Self
    where
        F: Fn(String, &mut WindowContext) + 'static,
    {
        Self {
            value: String::new(),
            on_search: Box::new(on_search),
        }
    }
}

impl RenderOnce for SearchBar {
    fn render(self, _cx: &mut WindowContext) -> impl IntoElement {
        let on_search = self.on_search;

        div()
            .h(px(48.0))
            .flex()
            .items_center()
            .px(px(16.0))
            .bg(rgb(0x252525))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.0))
                    .items_center()
                    .w_full()
                    .child("🔍")
                    .child(
                        input()
                            .placeholder("搜索...")
                            .value(self.value)
                            .flex_1()
                            .on_change(move |value: String, cx| {
                                on_search(value, cx);
                            })
                    )
            )
    }
}
```

**Step 2: 添加搜索过滤功能到数据库**

```rust
// Add to src/db.rs
pub fn search_records(&self, query: &str) -> Result<Vec<Record>> {
    let pattern = format!("%{}%", query);
    let mut stmt = self.conn.prepare(
        "SELECT id, content, created_at, record_type, priority, started_at, completed_at, source_record_id
         FROM records WHERE content LIKE ?1 ORDER BY created_at DESC"
    )?;

    let records = stmt.query_map([&pattern], |row| {
        Self::row_to_record(row)
    })?.collect::<Result<Vec<_>, _>>()?;

    Ok(records)
}
```

**Step 3: 添加搜索命令到 store**

```rust
// Add to src/store.rs
StoreCommand::SearchRecords(String),
StoreResponse::Records(Vec<Record>),

// In handle_command:
StoreCommand::SearchRecords(query) => {
    match db.search_records(&query) {
        Ok(records) => StoreResponse::Records(records),
        Err(e) => StoreResponse::Error(e.to_string()),
    }
}

// In Store impl:
pub async fn search_records(&self, query: &str) -> Result<Vec<Record>> {
    let (tx, rx) = async_channel::bounded(1);
    self.sender.send((StoreCommand::SearchRecords(query.to_string()), tx)).await?;

    match rx.recv().await? {
        StoreResponse::Records(records) => Ok(records),
        StoreResponse::Error(e) => Err(anyhow::anyhow!(e)),
        _ => Err(anyhow::anyhow!("Unexpected response")),
    }
}
```

**Step 4: 修改 main.rs 集成搜索栏**

```rust
// In MainView render:
div()
    .size_full()
    .flex()
    .flex_col()
    .bg(rgb(0x1a1a1a))
    .text_color(rgb(0xffffff))
    .child(
        SearchBar::new(cx.listener(|this, query: String, cx| {
            this.handle_search(query, cx);
        }))
    )
    .child(
        div()
            .flex_1()
            .flex()
            .child(Sidebar::new(...))
            .child(content_area)
    )
```

**Step 5: Commit**

```bash
git add src/ui/search_bar.rs src/db.rs src/store.rs src/main.rs
git commit -m "feat: add search bar with database search"
```

---

### Task 11: 创建 AI 面板（基础框架）

**Files:**
- Create: `src/ui/ai_panel.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/main.rs`

**Step 1: 创建 AI 面板**

```rust
// src/ui/ai_panel.rs
use crate::store::Store;
use gpui::*;

pub struct AiPanel {
    store: Store,
    input_value: String,
    response: String,
}

impl AiPanel {
    pub fn new(store: Store, _cx: &mut ViewContext<Self>) -> Self {
        Self {
            store,
            input_value: String::new(),
            response: "AI 总结功能将在后续版本实现。\n\n计划支持：\n- 时间段工作总结\n- 任务完成分析\n- 工作模式洞察".to_string(),
        }
    }
}

impl Render for AiPanel {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let input_value = self.input_value.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("AI 助手")
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        input()
                            .placeholder("询问 AI 关于你的工作...")
                            .value(input_value)
                            .on_change(cx.listener(|this, value: String, _cx| {
                                this.input_value = value;
                            }))
                    )
                    .child(button("发送"))
            )
            .child(
                div()
                    .flex_1()
                    .p(px(16.0))
                    .rounded(px(8.0))
                    .bg(rgb(0x252525))
                    .child(self.response.clone())
            )
    }
}
```

**Step 2: 修改 main.rs 集成**

```rust
use ui::ai_panel::AiPanel;

// In render match:
Panel::Ai => cx.new_view(|cx| AiPanel::new(self.store.clone(), cx)).into_any_element(),
```

**Step 3: Commit**

```bash
git add src/ui/ai_panel.rs src/ui/mod.rs src/main.rs
git commit -m "feat: add AI panel with placeholder interface"
```

---

## Phase 7: 全局快捷键

### Task 12: 实现全局快捷键

**Files:**
- Create: `src/shortcuts.rs`
- Modify: `src/main.rs`

**Step 1: 创建快捷键模块**

```rust
// src/shortcuts.rs
use gpui::*;

#[derive(Clone, Copy, Debug)]
pub enum ShortcutAction {
    ShowTasks,
    ShowRecords,
    ShowTimeline,
    ShowAi,
    ShowSearch,
    ShowSettings,
}

pub fn init_shortcuts(cx: &mut AppContext) {
    // Register global shortcuts
    cx.bind_keys([
        KeyBinding::new("cmd-1", ShortcutAction::ShowTasks, None),
        KeyBinding::new("cmd-2", ShortcutAction::ShowRecords, None),
        KeyBinding::new("cmd-3", ShortcutAction::ShowTimeline, None),
        KeyBinding::new("cmd-4", ShortcutAction::ShowAi, None),
        KeyBinding::new("cmd-k", ShortcutAction::ShowSearch, None),
        KeyBinding::new("cmd-," , ShortcutAction::ShowSettings, None),
    ]);
}

impl_actions!(shortcut, [ShortcutAction]);
```

**Step 2: 修改 main.rs 注册快捷键**

```rust
mod shortcuts;
use shortcuts::{init_shortcuts, ShortcutAction};

fn main() {
    App::new().run(|cx: &mut AppContext| {
        init_shortcuts(cx);

        // Subscribe to shortcut actions
        cx.subscribe_global::<ShortcutAction>(|action, cx| {
            // Handle shortcut action
            if let Some(window) = cx.active_window() {
                window.update(cx, |_, cx| {
                    // Dispatch action to main view
                }).ok();
            }
        });

        // ... rest of initialization
    });
}
```

**Step 3: 根据 GPUI 实际 API 调整**

GPUI 的全局快捷键 API 可能需要调整，根据实际编译错误修改。

**Step 4: Commit**

```bash
git add src/shortcuts.rs src/main.rs
git commit -m "feat: add global keyboard shortcuts"
```

---

## Phase 8: 设置面板和配置

### Task 13: 创建设置面板

**Files:**
- Create: `src/ui/settings_panel.rs`
- Create: `src/config.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/main.rs`

**Step 1: 创建配置模块**

```rust
// src/config.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub shortcuts: ShortcutsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutsConfig {
    pub tasks: String,
    pub records: String,
    pub timeline: String,
    pub ai: String,
    pub search: String,
    pub settings: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shortcuts: ShortcutsConfig {
                tasks: "cmd-1".to_string(),
                records: "cmd-2".to_string(),
                timeline: "cmd-3".to_string(),
                ai: "cmd-4".to_string(),
                search: "cmd-k".to_string(),
                settings: "cmd-,".to_string(),
            },
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config_path = Self::config_path()?;
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = serde_json::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let config_path = Self::config_path()?;
        std::fs::create_dir_all(config_path.parent().unwrap())?;
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    fn config_path() -> anyhow::Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("beitang");
        Ok(config_dir.join("config.json"))
    }
}
```

**Step 2: 创建设置面板**

```rust
// src/ui/settings_panel.rs
use crate::config::Config;
use gpui::*;

pub struct SettingsPanel {
    config: Config,
}

impl SettingsPanel {
    pub fn new(_cx: &mut ViewContext<Self>) -> Self {
        let config = Config::load().unwrap_or_default();
        Self { config }
    }

    fn save_config(&mut self, cx: &mut ViewContext<Self>) {
        if let Err(e) = self.config.save() {
            eprintln!("Failed to save config: {}", e);
        }
        cx.emit(DismissEvent);
    }
}

impl Render for SettingsPanel {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let shortcuts = self.config.shortcuts.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .p(px(24.0))
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("设置")
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child("快捷键")
                    .child(format!("任务面板: {}", shortcuts.tasks))
                    .child(format!("记录面板: {}", shortcuts.records))
                    .child(format!("时间线面板: {}", shortcuts.timeline))
                    .child(format!("AI 面板: {}", shortcuts.ai))
                    .child(format!("搜索: {}", shortcuts.search))
                    .child(format!("设置: {}", shortcuts.settings))
            )
            .child(
                div()
                    .mt_auto()
                    .child(
                        button("保存并关闭")
                            .on_click(cx.listener(|this, _event: &ClickEvent, cx| {
                                this.save_config(cx);
                            }))
                    )
            )
    }
}
```

**Step 3: 修改 main.rs 集成设置面板**

```rust
mod config;
use ui::settings_panel::SettingsPanel;

// In render, add settings drawer overlay when active
```

**Step 4: Commit**

```bash
git add src/config.rs src/ui/settings_panel.rs src/ui/mod.rs src/main.rs
git commit -m "feat: add settings panel with config persistence"
```

---

## Phase 9: 收尾和优化

### Task 14: 添加应用图标和打包配置

**Files:**
- Create: `assets/icon.png`
- Modify: `Cargo.toml`

**Step 1: 配置 macOS 应用 bundle**

```toml
# Add to Cargo.toml
[package.metadata.bundle]
name = "Beitang"
identifier = "com.yourname.beitang"
version = "0.1.0"
resources = ["assets"]
icon = ["assets/icon.png"]
category = "Productivity"
short_description = "工作中的闪念大脑"
long_description = """
超快记录工作事务（任务、想法、事件），
方便追溯、搜索、AI 总结。
"""
```

**Step 2: Commit**

```bash
git add Cargo.toml assets/
git commit -m "chore: add app bundle configuration"
```

---

### Task 15: 最终验证

**Step 1: 运行所有测试**

Run: `cargo test`
Expected: All tests pass

**Step 2: 构建 Release**

Run: `cargo build --release`
Expected: Builds successfully

**Step 3: 运行应用验证**

Run: `cargo run`
Expected: Application opens with all panels working

**Step 4: 最终提交**

```bash
git commit -m "chore: final verification and release build"
```

---

## 执行选项

**计划完成并保存到 `docs/plans/2026-03-01-rebuild-implementation-plan.md`。两个执行选项：**

**1. Subagent-Driven（本会话）** - 我为每个任务分派全新 subagent，任务间进行代码审查，快速迭代

**2. Parallel Session（独立会话）** - 开启新会话使用 executing-plans，批量执行带检查点

**选择哪种方式？**
