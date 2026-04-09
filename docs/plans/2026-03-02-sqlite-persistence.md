# SQLite 持久化实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将内存 mock 的 store 替换为真实的 SQLite 持久化存储，实现任务数据的保存和读取。

**Architecture:** 保留现有的异步 channel 架构（Store/Runtime 模式），在 Runtime 中初始化 SQLite 连接并执行真实的数据库操作。

**Tech Stack:** SQLite (rusqlite), async-channel, tokio

---

## 当前状态分析

- `src/store.rs` - 目前是 mock 实现，只返回空数据
- `src/db.rs` - 已有 Database 结构体，但未在 StoreRuntime 中使用
- 数据流：UI → Store (Sender) → Runtime (Receiver) → (目前是 mock) → 返回结果

---

## Task 1: 在 Runtime 中初始化数据库连接

**Files:**
- Modify: `src/store.rs:16-40`

**Step 1: 添加 Database 字段到 StoreRuntime**

修改 `src/store.rs`:

```rust
use crate::db::Database;
use std::path::PathBuf;

pub struct StoreRuntime {
    receiver: Receiver<StoreCommand>,
    db: Option<Database>, // 添加数据库连接
}

impl StoreRuntime {
    pub fn new(receiver: Receiver<StoreCommand>) -> Self {
        Self {
            receiver,
            db: None,
        }
    }

    pub async fn run(&mut self, db_path: PathBuf) {
        // 初始化数据库连接
        match Database::new(&db_path) {
            Ok(db) => {
                self.db = Some(db);
                println!("[Store] Database initialized at {:?}", db_path);
            }
            Err(e) => {
                eprintln!("[Store] Failed to initialize database: {}", e);
                return; // 数据库初始化失败，退出 runtime
            }
        }

        // 处理命令循环
        while let Ok(cmd) = self.receiver.recv().await {
            match cmd {
                StoreCommand::GetTasks { completed, respond_to } => {
                    let result = self.handle_get_tasks(completed).await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::CreateRecord { record, respond_to } => {
                    let result = self.handle_create_record(record).await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::UpdateRecord { record, respond_to } => {
                    let result = self.handle_update_record(record).await;
                    let _ = respond_to.send(result).await;
                }
            }
        }
    }

    // 处理方法（占位符，在 Task 2 中实现）
    async fn handle_get_tasks(&self, completed: bool) -> Result<Vec<Record>, String> {
        // Task 2 实现
        Ok(Vec::new())
    }

    async fn handle_create_record(&self, record: Record) -> Result<(), String> {
        // Task 2 实现
        Ok(())
    }

    async fn handle_update_record(&self, record: Record) -> Result<(), String> {
        // Task 2 实现
        Ok(())
    }
}
```

**Step 2: 修改 create_store 返回的 runtime 可变**

```rust
pub fn create_store() -> (Store, StoreRuntime) {
    let (sender, receiver) = unbounded();
    let store = Store { sender };
    let runtime = StoreRuntime::new(receiver);
    (store, runtime)  // runtime 不是 Arc，直接返回
}
```

**Step 3: 编译检查**

```bash
cargo check
```

Expected: 可能有错误，因为我们还没有实现 handler 方法

**Step 4: Commit**

```bash
git add src/store.rs
git commit -m "refactor: 准备 StoreRuntime 结构以支持数据库连接"
```

---

## Task 2: 实现真实的数据库操作方法

**Files:**
- Modify: `src/store.rs:50-80` (handler 方法)

**Step 1: 实现 handle_get_tasks**

```rust
async fn handle_get_tasks(&self, _completed: bool) -> Result<Vec<Record>, String> {
    match &self.db {
        Some(db) => {
            // 暂时返回所有任务，后续可以按 completed 过滤
            db.get_tasks(false)
                .map_err(|e| format!("Database error: {}", e))
        }
        None => Err("Database not initialized".to_string()),
    }
}
```

**Step 2: 实现 handle_create_record**

```rust
async fn handle_create_record(&self, record: Record) -> Result<(), String> {
    match &self.db {
        Some(db) => {
            db.create_record(&record)
                .map_err(|e| format!("Database error: {}", e))
        }
        None => Err("Database not initialized".to_string()),
    }
}
```

**Step 3: 实现 handle_update_record**

```rust
async fn handle_update_record(&self, record: Record) -> Result<(), String> {
    match &self.db {
        Some(db) => {
            db.create_record(&record)  // UPSERT 语义
                .map_err(|e| format!("Database error: {}", e))
        }
        None => Err("Database not initialized".to_string()),
    }
}
```

**Step 4: 编译检查**

```bash
cargo check
```

Expected: 应该通过

**Step 5: Commit**

```bash
git add src/store.rs
git commit -m "feat: 实现数据库操作方法"
```

---

## Task 3: 修复 main.rs 中的 runtime 调用

**Files:**
- Modify: `src/main.rs:26-34`

**Step 1: 修改 main 函数中的 runtime 启动**

当前代码：
```rust
let (store, runtime) = create_store();

// Spawn store runtime in background
cx.spawn(|_cx: &mut AsyncApp| async move {
    // ...
    runtime.run(db_path).await.ok();
}).detach();
```

需要改为：
```rust
let (store, mut runtime) = create_store();  // mut runtime

// Spawn store runtime in background
cx.spawn(|_cx: &mut AsyncApp| async move {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("robinne");
    std::fs::create_dir_all(&data_dir).ok();

    let db_path = data_dir.join("data.db");

    // 使用 &mut runtime 因为 run 需要 &mut self
    runtime.run(db_path).await;
}).detach();
```

**Step 2: 编译检查**

```bash
cargo check
```

Expected: 应该通过

**Step 3: 运行测试**

```bash
cargo run
```

手动测试：
1. 在输入框输入 "测试任务"
2. 按回车或点击"添加"按钮
3. 检查任务是否出现在列表中
4. 重启应用，检查任务是否还在

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "fix: 更新 main.rs 以使用新的可变 runtime"
```

---

## Task 4: 修复 Database 中的类型问题

**Files:**
- Modify: `src/db.rs:44-56` (优先级处理)

**Step 1: 修复 priority 的 Option<i64> 处理问题**

当前代码使用字符串拼接，应该使用 rusqlite 的 ToSql trait。

修改 `create_record` 方法中的参数绑定：

```rust
let priority_val: Option<i64> = record.priority.as_ref().map(|p| match p {
    Priority::High => 0,
    Priority::Medium => 1,
    Priority::Low => 2,
});

self.conn.execute(
    "INSERT INTO records (...)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
     ON CONFLICT(id) DO UPDATE SET ...",
    [
        &record.id.to_string(),
        &record.content,
        &priority_val,  // Option<i64> 自动处理
        &record.created_at.to_rfc3339(),
        &record.completed_at.map(|t| t.to_rfc3339()),
        &match record.record_type {
            RecordType::Task => "task",
            RecordType::Note => "note",
            RecordType::Event => "event",
        },
    ],
)?;
```

**Step 2: 修复 get_tasks 中的空字符串问题**

如果 completed_at 是空字符串，应该返回 None：

```rust
completed_at: completed_at_str
    .filter(|s| !s.is_empty())
    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
    .map(|d| d.with_timezone(&Utc)),
```

**Step 3: 编译检查**

```bash
cargo check
```

**Step 4: 运行完整测试**

```bash
cargo run
```

**Step 5: Commit**

```bash
git add src/db.rs
git commit -m "fix: 修复数据库类型处理问题"
```

---

## Task 5: 添加错误处理和日志

**Files:**
- Modify: `src/store.rs`

**Step 1: 添加更好的错误日志**

在每个 handler 中添加日志：

```rust
async fn handle_create_record(&self, record: Record) -> Result<(), String> {
    println!("[Store] Creating record: {}", record.id);
    match &self.db {
        Some(db) => {
            match db.create_record(&record) {
                Ok(_) => {
                    println!("[Store] Record created successfully");
                    Ok(())
                }
                Err(e) => {
                    eprintln!("[Store] Failed to create record: {}", e);
                    Err(format!("Database error: {}", e))
                }
            }
        }
        None => {
            eprintln!("[Store] Database not initialized");
            Err("Database not initialized".to_string())
        }
    }
}
```

**Step 2: 编译并测试**

```bash
cargo check && cargo run
```

**Step 3: Commit**

```bash
git add src/store.rs
git commit -m "chore: 添加存储层日志"
```

---

## 验证清单

- [ ] 创建任务后能在列表中看到
- [ ] 任务优先级正确显示（🔴🟡🟢）
- [ ] 重启应用后任务仍然存在
- [ ] 完成任务后状态被保存
- [ ] 数据库文件创建在 `~/Library/Application Support/robinne/data.db`

---

## 相关文件

- `src/store.rs` - Store 和 StoreRuntime 实现
- `src/db.rs` - Database 结构体（已存在）
- `src/models.rs` - Record, Priority, RecordType 定义
- `src/main.rs` - 应用入口，runtime 启动

## 依赖

```toml
[dependencies]
rusqlite = { version = "0.30", features = ["bundled", "chrono"] }
dirs = "5.0"
```

（这些依赖已在 Cargo.toml 中）
