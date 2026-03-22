# 北堂 (Beitang) - 数据模型设计文档

**文档日期**: 2026-03-22  
**版本**: 1.0  
**数据库**: SQLite + FTS5

---

## 一、设计原则

1. **统一存储**: Task、Event、Idea 共用主表，通过 `record_type` 区分
2. **查询性能**: 关键字段建立索引，支持毫秒级查询
3. **全文搜索**: 使用 SQLite FTS5 实现高效搜索
4. **本地优先**: 所有数据本地存储，附件独立目录
5. **无孤儿数据**: 删除记录级联删除关联数据，定期清理孤儿附件

---

## 二、表结构

### 2.1 主表: records

存储所有记录（任务、事件、想法）。

```sql
CREATE TABLE records (
    id TEXT PRIMARY KEY,              -- UUID v4
    created_at DATETIME NOT NULL,     -- 创建时间（时间轴位置）
    updated_at DATETIME NOT NULL,     -- 最后更新时间
    
    content TEXT NOT NULL,            -- 正文内容
    
    -- 类型区分
    record_type TEXT CHECK(record_type IN ('Task', 'Event', 'Idea')),
    
    -- Task 专属字段（Event/Idea 为 NULL）
    status TEXT CHECK(status IN ('Todo', 'InProgress', 'Done', 'Cancelled')),
    priority TEXT CHECK(priority IN ('High', 'Medium', 'Low')),
    due_date DATETIME,                -- DDL（截止日期）
    reminder_at DATETIME,             -- 提醒时间
    completed_at DATETIME,            -- 完成/取消时间
    cancelled_reason TEXT             -- 取消原因（可选）
);
```

**字段说明**:

| 字段 | 类型 | 说明 | 可空 |
|------|------|------|------|
| `id` | TEXT | UUID，主键 | 否 |
| `created_at` | DATETIME | 创建时间，决定时间轴位置 | 否 |
| `updated_at` | DATETIME | 更新时间 | 否 |
| `content` | TEXT | 正文内容 | 否 |
| `record_type` | TEXT | Task/Event/Idea | 否 |
| `status` | TEXT | 任务状态 | 是（仅 Task）|
| `priority` | TEXT | 优先级 High/Medium/Low | 是（仅 Task）|
| `due_date` | DATETIME | DDL | 是 |
| `reminder_at` | DATETIME | 提醒时间 | 是 |
| `completed_at` | DATETIME | 完成时间 | 是 |
| `cancelled_reason` | TEXT | 取消原因 | 是 |

---

### 2.2 全文搜索表: records_fts

使用 SQLite FTS5 虚拟表，自动同步主表内容。

```sql
-- 创建 FTS5 虚拟表
CREATE VIRTUAL TABLE records_fts USING fts5(
    content,                          -- 要索引的字段
    content='records',               -- 关联的主表
    content_rowid='rowid'             -- 关联的字段
);

-- 触发器：保持 FTS 表与主表同步
CREATE TRIGGER records_fts_insert AFTER INSERT ON records BEGIN
    INSERT INTO records_fts(rowid, content) VALUES (new.rowid, new.content);
END;

CREATE TRIGGER records_fts_delete AFTER DELETE ON records BEGIN
    INSERT INTO records_fts(records_fts, rowid, content) VALUES('delete', old.rowid, old.content);
END;

CREATE TRIGGER records_fts_update AFTER UPDATE ON records BEGIN
    INSERT INTO records_fts(records_fts, rowid, content) VALUES('delete', old.rowid, old.content);
    INSERT INTO records_fts(rowid, content) VALUES (new.rowid, new.content);
END;
```

---

### 2.3 标签系统

#### tags 表（标签定义）

```sql
CREATE TABLE tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT UNIQUE NOT NULL,        -- 标签名（不含#）
    color TEXT,                       -- 预留：标签颜色
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

#### record_tags 表（记录-标签关联）

```sql
CREATE TABLE record_tags (
    record_id TEXT NOT NULL,
    tag_id INTEGER NOT NULL,
    PRIMARY KEY (record_id, tag_id),
    FOREIGN KEY (record_id) REFERENCES records(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);
```

---

### 2.4 人物关联系统

与标签系统结构相同，独立存储。

#### persons 表

```sql
CREATE TABLE persons (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT UNIQUE NOT NULL,        -- 人物名（不含@）
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

#### record_persons 表

```sql
CREATE TABLE record_persons (
    record_id TEXT NOT NULL,
    person_id INTEGER NOT NULL,
    PRIMARY KEY (record_id, person_id),
    FOREIGN KEY (record_id) REFERENCES records(id) ON DELETE CASCADE,
    FOREIGN KEY (person_id) REFERENCES persons(id) ON DELETE CASCADE
);
```

---

### 2.5 附件系统

#### attachments 表（元数据）

```sql
CREATE TABLE attachments (
    id TEXT PRIMARY KEY,              -- UUID
    record_id TEXT NOT NULL,
    file_name TEXT NOT NULL,          -- 原始文件名
    file_path TEXT NOT NULL,          -- 相对路径（attachments/年/月/uuid.ext）
    file_size INTEGER,                -- 字节数
    mime_type TEXT,                   -- image/png, image/jpeg
    width INTEGER,                    -- 图片宽度
    height INTEGER,                   -- 图片高度
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (record_id) REFERENCES records(id) ON DELETE CASCADE
);
```

**物理存储路径**:
```
~/Library/Application Support/Beitang/
├── beitang.db                 # SQLite 主数据库
├── beitang.db-shm             # WAL 模式文件
├── beitang.db-wal
└── attachments/               # 附件目录
    ├── 2026/
    │   ├── 03/
    │   │   ├── a1b2c3d4.png   # 截图（PNG）
    │   │   └── e5f6g7h8.jpg   # 照片（JPEG 压缩后）
    └── temp/                  # 临时文件（定期清理）
```

---

## 三、索引设计

### 3.1 已创建索引

```sql
-- 时间线查询（最重要）
CREATE INDEX idx_records_created_at ON records(created_at DESC);

-- 任务状态查询（待办列表）
CREATE INDEX idx_records_task_status ON records(status) 
    WHERE record_type = 'Task';

-- 任务排序（四象限展示）
CREATE INDEX idx_records_task_due ON records(due_date, priority) 
    WHERE record_type = 'Task' AND status IN ('Todo', 'InProgress');

-- 标签查询优化
CREATE INDEX idx_record_tags_record ON record_tags(record_id);
CREATE INDEX idx_record_tags_tag ON record_tags(tag_id);

-- 人物查询优化
CREATE INDEX idx_record_persons_record ON record_persons(record_id);
CREATE INDEX idx_record_persons_person ON record_persons(person_id);

-- 附件查询
CREATE INDEX idx_attachments_record ON attachments(record_id);
```

### 3.2 索引说明

| 索引 | 用途 | 场景 |
|------|------|------|
| `idx_records_created_at` | 时间线查询 | 首页回顾、时间线面板 |
| `idx_records_task_status` | 任务筛选 | 待办列表、进行中任务 |
| `idx_records_task_due` | 四象限排序 | 按 DDL+优先级排序 |

---

## 四、核心查询示例

### 4.1 时间线查询（最近 N 条）

```sql
SELECT r.*, 
       GROUP_CONCAT(DISTINCT t.name) as tags,
       GROUP_CONCAT(DISTINCT p.name) as persons
FROM records r
LEFT JOIN record_tags rt ON r.id = rt.record_id
LEFT JOIN tags t ON rt.tag_id = t.id
LEFT JOIN record_persons rp ON r.id = rp.record_id
LEFT JOIN persons p ON rp.person_id = p.id
WHERE r.created_at < ?  -- 上一页最后一条时间
GROUP BY r.id
ORDER BY r.created_at DESC
LIMIT 50;
```

### 4.2 待办任务查询（四象限排序）

```sql
SELECT r.*,
       CASE 
           WHEN r.due_date IS NOT NULL AND r.due_date <= datetime('now', '+1 day') THEN 1
           ELSE 0
       END as is_urgent,
       CASE 
           WHEN r.priority = 'High' THEN 1
           WHEN r.priority = 'Medium' THEN 2
           ELSE 3
       END as priority_order
FROM records r
WHERE r.record_type = 'Task' 
  AND r.status IN ('Todo', 'InProgress')
ORDER BY 
    is_urgent ASC,
    priority_order ASC,
    r.due_date ASC;
```

### 4.3 全文搜索

```sql
-- 搜索内容
SELECT r.* FROM records r
JOIN records_fts fts ON r.rowid = fts.rowid
WHERE records_fts MATCH ?
ORDER BY r.created_at DESC;

-- 搜索 + 标签筛选
SELECT r.* FROM records r
JOIN records_fts fts ON r.rowid = fts.rowid
JOIN record_tags rt ON r.id = rt.record_id
JOIN tags t ON rt.tag_id = t.id
WHERE records_fts MATCH ?
  AND t.name IN ('工作', '项目A')
GROUP BY r.id
ORDER BY r.created_at DESC;
```

### 4.4 标签列表查询

```sql
-- 获取所有标签及使用次数
SELECT t.*, COUNT(rt.record_id) as count
FROM tags t
LEFT JOIN record_tags rt ON t.id = rt.tag_id
GROUP BY t.id
ORDER BY count DESC, t.name;
```

---

## 五、附件处理策略

### 5.1 图片压缩规则

| 图片来源 | 处理方式 | 输出格式 | 最大尺寸 |
|---------|---------|---------|---------|
| 截图（系统截屏）| 保留原图或 oxipng 无损优化 | PNG | 原尺寸 |
| 照片（相机/手机）| 等比缩放 + 质量压缩 85% | JPEG | 1920px 宽边 |
| 文档扫描 | 灰度 + 高压缩 | JPEG | 1080px 宽边 |

### 5.2 文件命名规则

```
{uuid}.{ext}

示例:
- a1b2c3d4-e5f6-7890-abcd-ef1234567890.png
- b2c3d4e5-f6g7-8901-bcde-f23456789012.jpg
```

### 5.3 清理策略

- **删除记录时**: 级联删除 attachments 表记录，但**不立即删除物理文件**（防误删）
- **定期清理**: 启动时检查孤儿文件（不在 attachments 表中的文件），移动到 trash 目录
- **缓存目录**: `attachments/temp/` 存放临时文件，每次启动清空

---

## 六、数据迁移与版本管理

### 6.1 数据库版本表

```sql
CREATE TABLE schema_version (
    version INTEGER PRIMARY KEY,
    applied_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    description TEXT
);
```

### 6.2 迁移策略

- 应用启动时检查 `schema_version`
- 按需执行增量迁移脚本
- FTS5 表自动重建（schema 变更时）

---

## 七、性能考虑

### 7.1 查询优化

- 时间线查询使用覆盖索引
- 标签/人物关联使用延迟加载（分页查询时不 JOIN，详情页才加载）
- 全文搜索使用 FTS5 的 `MATCH` 而非 `LIKE`

### 7.2 大数据量处理

- 时间线虚拟滚动，只渲染可见区域
- 增量加载（每次 50 条）
- 图片懒加载（进入视口才加载）

### 7.3 WAL 模式

启用 SQLite WAL 模式提升并发性能：

```sql
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
```

---

## 八、Rust 结构体对应

```rust
// Record 主结构
pub struct Record {
    pub id: String,                    // UUID
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub content: String,
    pub record_type: RecordType,
    
    // Task 专属
    pub status: Option<TaskStatus>,
    pub priority: Option<Priority>,
    pub due_date: Option<DateTime<Utc>>,
    pub reminder_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub cancelled_reason: Option<String>,
    
    // 关联（非数据库字段，查询时填充）
    pub tags: Vec<String>,
    pub persons: Vec<String>,
    pub attachments: Vec<Attachment>,
}

pub enum RecordType {
    Task,
    Event,
    Idea,
}

pub enum TaskStatus {
    Todo,
    InProgress,
    Done,
    Cancelled,
}

pub enum Priority {
    High,    // !!
    Medium,  // !
    Low,
}

pub struct Attachment {
    pub id: String,
    pub file_name: String,
    pub file_path: String,
    pub file_size: usize,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
}
```

---

## 九、待确认事项

- [ ] 是否需要 `schema_migrations` 表记录详细迁移历史
- [ ] 附件文件是否需要在数据库中存储 MD5 校验和
- [ ] 是否需要 `record_links` 表支持记录间关联（预留功能）
- [ ] 软删除 vs 硬删除（当前设计为硬删除 + 级联）

---

**相关文档**:
- [产品设计方案](./product-design.md)
- [UI/UX 设计文档](./ui-ux-design.md)
