use crate::models::{Attachment, Person, Priority, Record, RecordType, Tag, TaskStatus};
use chrono::{DateTime, Utc};
use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection, Result};
use uuid::Uuid;

const SCHEMA_VERSION: i64 = 4;
const SEARCH_RANK_FALLBACK: f64 = 1_000_000_000.0;

#[derive(Debug, Clone)]
struct SearchToken {
    like_pattern: String,
    fts_prefix_query: Option<String>,
}

fn escape_like_pattern(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn tokenize_search_query(query: &str) -> Vec<SearchToken> {
    query
        .trim()
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(|token| SearchToken {
            like_pattern: format!("%{}%", escape_like_pattern(token)),
            fts_prefix_query: if token.chars().count() >= 2 {
                Some(format!("\"{}\"*", token.replace('"', "\"\"")))
            } else {
                None
            },
        })
        .collect()
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init_tables()?;
        db.run_migrations()?;
        Ok(db)
    }

    fn init_tables(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                updated_at TEXT NOT NULL
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS records (
                id TEXT PRIMARY KEY,
                title TEXT,
                content TEXT NOT NULL,
                priority INTEGER,
                status TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                completed_at TEXT,
                scheduled_for TEXT,
                due_date TEXT,
                notified_at TEXT,
                cancelled_reason TEXT,
                record_type TEXT NOT NULL
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS tags (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                color TEXT,
                created_at TEXT NOT NULL
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS record_tags (
                record_id TEXT NOT NULL,
                tag_id INTEGER NOT NULL,
                PRIMARY KEY (record_id, tag_id),
                FOREIGN KEY (record_id) REFERENCES records(id) ON DELETE CASCADE,
                FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS persons (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS record_persons (
                record_id TEXT NOT NULL,
                person_id INTEGER NOT NULL,
                PRIMARY KEY (record_id, person_id),
                FOREIGN KEY (record_id) REFERENCES records(id) ON DELETE CASCADE,
                FOREIGN KEY (person_id) REFERENCES persons(id) ON DELETE CASCADE
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS attachments (
                id TEXT PRIMARY KEY,
                record_id TEXT NOT NULL,
                file_name TEXT NOT NULL,
                file_path TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                mime_type TEXT NOT NULL,
                width INTEGER,
                height INTEGER,
                created_at TEXT NOT NULL,
                FOREIGN KEY (record_id) REFERENCES records(id) ON DELETE CASCADE
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_records_type ON records(record_type)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_records_created_at ON records(created_at DESC)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_records_updated_at ON records(updated_at DESC)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_records_completed_at ON records(completed_at)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_records_scheduled_for ON records(scheduled_for)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_records_due_date ON records(due_date)",
            [],
        )?;
        self.conn
            .execute("CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name)", [])?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_record_tags_tag_id ON record_tags(tag_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_persons_name ON persons(name)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_record_persons_person_id ON record_persons(person_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_attachments_record_id ON attachments(record_id)",
            [],
        )?;

        self.create_records_fts_schema()?;

        Ok(())
    }

    fn create_records_fts_schema(&self) -> Result<()> {
        self.conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS records_fts USING fts5(
                record_id,
                content
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS records_fts_insert
             AFTER INSERT ON records
             BEGIN
                 INSERT INTO records_fts (record_id, content)
                 VALUES (new.id, COALESCE(new.title, '') || ' ' || COALESCE(new.content, ''));
             END",
            [],
        )?;

        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS records_fts_update
             AFTER UPDATE ON records
             BEGIN
                 UPDATE records_fts
                 SET content = COALESCE(new.title, '') || ' ' || COALESCE(new.content, '')
                 WHERE record_id = old.id;
             END",
            [],
        )?;

        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS records_fts_delete
             AFTER DELETE ON records
             BEGIN
                 DELETE FROM records_fts WHERE record_id = old.id;
             END",
            [],
        )?;

        Ok(())
    }

    fn drop_records_fts_schema(&self) -> Result<()> {
        self.conn
            .execute("DROP TRIGGER IF EXISTS records_fts_insert", [])?;
        self.conn
            .execute("DROP TRIGGER IF EXISTS records_fts_update", [])?;
        self.conn
            .execute("DROP TRIGGER IF EXISTS records_fts_delete", [])?;
        self.conn.execute("DROP TABLE IF EXISTS records_fts", [])?;
        Ok(())
    }

    fn rebuild_records_fts(&self) -> Result<()> {
        self.conn.execute("DELETE FROM records_fts", [])?;
        self.conn.execute(
            "INSERT INTO records_fts (record_id, content)
             SELECT id, COALESCE(title, '') || ' ' || COALESCE(content, '')
             FROM records",
            [],
        )?;
        Ok(())
    }

    fn run_migrations(&self) -> Result<()> {
        let current_version: Option<i64> = self
            .conn
            .query_row(
                "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();

        let version = current_version.unwrap_or(0);

        if version < 1 {
            self.migrate_v0_to_v1()?;
        }

        if version < 2 {
            self.migrate_v1_to_v2()?;
        }

        if version < 3 {
            self.migrate_v2_to_v3()?;
        }

        if version < 4 {
            self.migrate_v3_to_v4()?;
        }

        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO schema_version (version, updated_at) VALUES (?1, ?2)
             ON CONFLICT(version) DO UPDATE SET updated_at = excluded.updated_at",
            [&SCHEMA_VERSION.to_string(), &now],
        )?;

        Ok(())
    }

    fn migrate_v0_to_v1(&self) -> Result<()> {
        let _ = self
            .conn
            .execute("ALTER TABLE records ADD COLUMN scheduled_for TEXT", []);
        let _ = self
            .conn
            .execute("ALTER TABLE records ADD COLUMN due_date TEXT", []);
        let _ = self
            .conn
            .execute("ALTER TABLE records ADD COLUMN notified_at TEXT", []);
        let _ = self
            .conn
            .execute("ALTER TABLE records ADD COLUMN status TEXT", []);
        let _ = self
            .conn
            .execute("ALTER TABLE records ADD COLUMN updated_at TEXT", []);
        let _ = self
            .conn
            .execute("ALTER TABLE records ADD COLUMN cancelled_reason TEXT", []);

        let _ = self.conn.execute(
            "UPDATE records SET updated_at = created_at WHERE updated_at IS NULL",
            [],
        );

        Ok(())
    }

    fn migrate_v1_to_v2(&self) -> Result<()> {
        Ok(())
    }

    fn migrate_v2_to_v3(&self) -> Result<()> {
        // 添加 title 列
        let _ = self
            .conn
            .execute("ALTER TABLE records ADD COLUMN title TEXT", []);

        // 迁移现有数据：将 content 第一行提取为 title，剩余内容保留在 content 中
        let mut stmt = self
            .conn
            .prepare("SELECT id, content FROM records WHERE title IS NULL")?;
        let rows: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>>>()?;

        for (id, content) in rows {
            let lines: Vec<&str> = content.lines().collect();

            if lines.is_empty() {
                continue;
            }

            // 第一行作为 title
            let title = lines[0].trim().to_string();

            // 剩余行作为新的 content（保留换行）
            let new_content = if lines.len() > 1 {
                lines[1..].join("\n").trim().to_string()
            } else {
                String::new()
            };

            self.conn.execute(
                "UPDATE records SET title = ?1, content = ?2 WHERE id = ?3",
                [&title, &new_content, &id],
            )?;
        }

        Ok(())
    }

    fn migrate_v3_to_v4(&self) -> Result<()> {
        self.drop_records_fts_schema()?;
        self.create_records_fts_schema()?;
        self.rebuild_records_fts()?;
        Ok(())
    }

    pub fn create_record(&self, record: &Record) -> Result<()> {
        let priority_val = record.priority.as_ref().map(|p| match p {
            Priority::High => 0i64,
            Priority::Medium => 1i64,
            Priority::Low => 2i64,
        });

        let status_str = record.status.as_ref().map(|s| match s {
            TaskStatus::Todo => "todo",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Done => "done",
            TaskStatus::Cancelled => "cancelled",
        });

        let record_type_str = match record.record_type {
            RecordType::Task => "task",
            RecordType::Note => "note",
            RecordType::Event => "event",
            RecordType::Idea => "idea",
        };

        eprintln!(
            "[DB] create_record: id={}, content='{}', priority={:?}",
            record.id, record.content, priority_val
        );

        let tx = self.conn.unchecked_transaction()?;

        tx.execute(
            "INSERT INTO records (id, title, content, priority, status, created_at, updated_at, completed_at, scheduled_for, due_date, notified_at, cancelled_reason, record_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                content = excluded.content,
                priority = excluded.priority,
                status = excluded.status,
                completed_at = excluded.completed_at,
                scheduled_for = excluded.scheduled_for,
                due_date = excluded.due_date,
                notified_at = excluded.notified_at,
                cancelled_reason = excluded.cancelled_reason,
                updated_at = excluded.updated_at",
            [
                &record.id.to_string() as &dyn rusqlite::ToSql,
                &record.title as &dyn rusqlite::ToSql,
                &record.content as &dyn rusqlite::ToSql,
                &priority_val as &dyn rusqlite::ToSql,
                &status_str as &dyn rusqlite::ToSql,
                &record.created_at.to_rfc3339() as &dyn rusqlite::ToSql,
                &record.updated_at.to_rfc3339() as &dyn rusqlite::ToSql,
                &record.completed_at.map(|t| t.to_rfc3339()) as &dyn rusqlite::ToSql,
                &record.scheduled_for.map(|t| t.to_rfc3339()) as &dyn rusqlite::ToSql,
                &record.due_date.map(|t| t.to_rfc3339()) as &dyn rusqlite::ToSql,
                &record.notified_at.map(|t| t.to_rfc3339()) as &dyn rusqlite::ToSql,
                &record.cancelled_reason as &dyn rusqlite::ToSql,
                &record_type_str as &dyn rusqlite::ToSql,
            ],
        )?;

        tx.execute(
            "DELETE FROM record_tags WHERE record_id = ?1",
            [&record.id.to_string()],
        )?;
        for tag_name in &record.tags {
            let tag_id = self.get_or_create_tag_internal(&tx, tag_name)?;
            tx.execute(
                "INSERT INTO record_tags (record_id, tag_id) VALUES (?1, ?2)",
                [
                    &record.id.to_string() as &dyn rusqlite::ToSql,
                    &tag_id as &dyn rusqlite::ToSql,
                ],
            )?;
        }

        tx.execute(
            "DELETE FROM record_persons WHERE record_id = ?1",
            [&record.id.to_string()],
        )?;
        for person_name in &record.persons {
            let person_id = self.get_or_create_person_internal(&tx, person_name)?;
            tx.execute(
                "INSERT INTO record_persons (record_id, person_id) VALUES (?1, ?2)",
                [
                    &record.id.to_string() as &dyn rusqlite::ToSql,
                    &person_id as &dyn rusqlite::ToSql,
                ],
            )?;
        }

        tx.commit()?;
        eprintln!("[DB] create_record succeeded");
        Ok(())
    }

    fn get_or_create_tag_internal(&self, tx: &rusqlite::Transaction, name: &str) -> Result<i64> {
        let now = Utc::now().to_rfc3339();

        tx.execute(
            "INSERT OR IGNORE INTO tags (name, created_at) VALUES (?1, ?2)",
            [name, &now],
        )?;

        let id: i64 = tx.query_row("SELECT id FROM tags WHERE name = ?1", [name], |row| {
            row.get(0)
        })?;

        Ok(id)
    }

    fn get_or_create_person_internal(&self, tx: &rusqlite::Transaction, name: &str) -> Result<i64> {
        let now = Utc::now().to_rfc3339();

        tx.execute(
            "INSERT OR IGNORE INTO persons (name, created_at) VALUES (?1, ?2)",
            [name, &now],
        )?;

        let id: i64 = tx.query_row("SELECT id FROM persons WHERE name = ?1", [name], |row| {
            row.get(0)
        })?;

        Ok(id)
    }

    pub fn get_tasks(&self, _completed: bool) -> Result<Vec<Record>> {
        eprintln!("[DB] get_tasks called");
        let mut stmt = self.conn.prepare(
            "SELECT id, title, content, priority, status, created_at, updated_at, completed_at, scheduled_for, due_date, notified_at, cancelled_reason, record_type
             FROM records
             WHERE record_type = 'task'
             ORDER BY created_at DESC"
        )?;

        let records = stmt.query_map([], |row| self.row_to_record(row))?;

        let mut result = Vec::new();
        for record in records {
            let mut record = record?;
            self.load_record_associations(&mut record)?;
            result.push(record);
        }

        eprintln!("[DB] get_tasks returning {} records", result.len());
        Ok(result)
    }

    pub fn get_notes(&self) -> Result<Vec<Record>> {
        eprintln!("[DB] get_notes called");
        let mut stmt = self.conn.prepare(
            "SELECT id, title, content, priority, status, created_at, updated_at, completed_at, scheduled_for, due_date, notified_at, cancelled_reason, record_type
             FROM records
             WHERE record_type = 'note'
             ORDER BY created_at DESC"
        )?;

        let records = stmt.query_map([], |row| self.row_to_record(row))?;

        let mut result = Vec::new();
        for record in records {
            let mut record = record?;
            self.load_record_associations(&mut record)?;
            result.push(record);
        }

        eprintln!("[DB] get_notes returning {} records", result.len());
        Ok(result)
    }

    pub fn get_records_by_tag(&self, tag: &str) -> Result<Vec<Record>> {
        eprintln!("[DB] get_records_by_tag called with tag='{}'", tag);
        let mut stmt = self.conn.prepare(
            "SELECT r.id, r.title, r.content, r.priority, r.status, r.created_at, r.updated_at,
                    r.completed_at, r.scheduled_for, r.due_date, r.notified_at,
                    r.cancelled_reason, r.record_type
             FROM records r
             JOIN record_tags rt ON r.id = rt.record_id
             JOIN tags t ON t.id = rt.tag_id
             WHERE t.name = ?1
             ORDER BY r.updated_at DESC",
        )?;

        let records = stmt.query_map([tag], |row| self.row_to_record(row))?;

        let mut result = Vec::new();
        for record in records {
            let mut record = record?;
            self.load_record_associations(&mut record)?;
            result.push(record);
        }

        eprintln!("[DB] get_records_by_tag returning {} records", result.len());
        Ok(result)
    }

    pub fn get_pending_reminders(&self) -> Result<Vec<Record>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, content, priority, status, created_at, updated_at, completed_at, scheduled_for, due_date, notified_at, cancelled_reason, record_type
             FROM records
             WHERE completed_at IS NULL
               AND notified_at IS NULL
               AND (
                   (scheduled_for IS NOT NULL AND scheduled_for <= ?1) OR
                   (due_date IS NOT NULL AND due_date <= ?2)
               )
             ORDER BY created_at ASC"
        )?;

        let now = Utc::now().to_rfc3339();

        let records = stmt.query_map([&now, &now], |row| self.row_to_record(row))?;

        let mut result = Vec::new();
        for record in records {
            let mut record = record?;
            self.load_record_associations(&mut record)?;
            result.push(record);
        }

        Ok(result)
    }

    pub fn delete_record(&self, id: Uuid) -> Result<()> {
        eprintln!("[DB] delete_record called for id: {}", id);
        self.conn
            .execute("DELETE FROM records WHERE id = ?1", [&id.to_string()])?;
        eprintln!("[DB] delete_record succeeded");
        Ok(())
    }

    pub fn mark_task_completed(&self, id: Uuid, completed_at: DateTime<Utc>) -> Result<()> {
        eprintln!("[DB] mark_task_completed called for id: {}", id);
        self.conn.execute(
            "UPDATE records SET completed_at = ?1, status = 'done', updated_at = ?2 WHERE id = ?3",
            [
                &completed_at.to_rfc3339() as &dyn rusqlite::ToSql,
                &Utc::now().to_rfc3339() as &dyn rusqlite::ToSql,
                &id.to_string() as &dyn rusqlite::ToSql,
            ],
        )?;
        Ok(())
    }

    pub fn get_timeline(&self, limit: i64, offset: i64) -> Result<Vec<Record>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, content, priority, status, created_at, updated_at, completed_at, scheduled_for, due_date, notified_at, cancelled_reason, record_type
             FROM records
             ORDER BY created_at DESC
             LIMIT ?1 OFFSET ?2"
        )?;

        let records = stmt.query_map([limit, offset], |row| self.row_to_record(row))?;

        let mut result = Vec::new();
        for record in records {
            let mut record = record?;
            self.load_record_associations(&mut record)?;
            result.push(record);
        }

        Ok(result)
    }

    pub fn search_records(&self, query: &str) -> Result<Vec<Record>> {
        let tokens = tokenize_search_query(query);
        if tokens.is_empty() {
            return Ok(Vec::new());
        }

        let title_score_fragments = tokens
            .iter()
            .map(|_| "CASE WHEN COALESCE(r.title, '') LIKE ? ESCAPE '\\' THEN 1 ELSE 0 END")
            .collect::<Vec<_>>()
            .join(" + ");
        let title_score_expr = if title_score_fragments.is_empty() {
            "0".to_string()
        } else {
            title_score_fragments
        };

        let fts_tokens: Vec<&str> = tokens
            .iter()
            .filter_map(|token| token.fts_prefix_query.as_deref())
            .collect();
        let fts_rank_expr = if fts_tokens.is_empty() {
            format!("{SEARCH_RANK_FALLBACK}")
        } else {
            "COALESCE((
                SELECT bm25(records_fts)
                FROM records_fts
                WHERE record_id = r.id AND records_fts MATCH ?
            ), ?)"
                .to_string()
        };

        let mut where_clauses = Vec::new();
        let mut params = Vec::new();

        for token in &tokens {
            params.push(Value::from(token.like_pattern.clone()));
        }

        if !fts_tokens.is_empty() {
            params.push(Value::from(fts_tokens.join(" ")));
            params.push(Value::from(SEARCH_RANK_FALLBACK));
        }

        for token in &tokens {
            if let Some(fts_prefix_query) = &token.fts_prefix_query {
                where_clauses.push(
                    "(EXISTS(
                        SELECT 1
                        FROM records_fts
                        WHERE record_id = r.id AND records_fts MATCH ?
                    ) OR COALESCE(r.title, '') LIKE ? ESCAPE '\\'
                      OR r.content LIKE ? ESCAPE '\\')"
                        .to_string(),
                );
                params.push(Value::from(fts_prefix_query.clone()));
                params.push(Value::from(token.like_pattern.clone()));
                params.push(Value::from(token.like_pattern.clone()));
            } else {
                where_clauses.push(
                    "(COALESCE(r.title, '') LIKE ? ESCAPE '\\'
                      OR r.content LIKE ? ESCAPE '\\')"
                        .to_string(),
                );
                params.push(Value::from(token.like_pattern.clone()));
                params.push(Value::from(token.like_pattern.clone()));
            }
        }

        let sql = format!(
            "SELECT r.id, r.title, r.content, r.priority, r.status, r.created_at, r.updated_at,
                    r.completed_at, r.scheduled_for, r.due_date, r.notified_at,
                    r.cancelled_reason, r.record_type,
                    ({title_score_expr}) AS title_match_count,
                    ({fts_rank_expr}) AS fts_rank
             FROM records r
             WHERE {}
             ORDER BY CASE WHEN title_match_count > 0 THEN 0 ELSE 1 END,
                      fts_rank,
                      r.updated_at DESC",
            where_clauses.join(" AND ")
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let records = stmt.query_map(params_from_iter(params.iter()), |row| {
            self.row_to_record(row)
        })?;

        let mut result = Vec::new();
        for record in records {
            let mut record = record?;
            self.load_record_associations(&mut record)?;
            result.push(record);
        }

        Ok(result)
    }

    pub fn create_tag(&self, name: &str, color: Option<&str>) -> Result<Tag> {
        let now = Utc::now();
        self.conn.execute(
            "INSERT INTO tags (name, color, created_at) VALUES (?1, ?2, ?3)",
            [name, color.unwrap_or(""), &now.to_rfc3339()],
        )?;

        let id = self.conn.last_insert_rowid();
        Ok(Tag {
            id,
            name: name.to_string(),
            color: color.map(|s| s.to_string()),
            created_at: now,
        })
    }

    pub fn get_tags(&self) -> Result<Vec<Tag>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, color, created_at FROM tags ORDER BY name")?;

        let tags = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let name: String = row.get(1)?;
            let color: Option<String> = row.get(2)?;
            let created_at_str: String = row.get(3)?;

            Ok(Tag {
                id,
                name,
                color,
                created_at: DateTime::parse_from_rfc3339(&created_at_str)
                    .unwrap_or_else(|_| chrono::Local::now().into())
                    .with_timezone(&Utc),
            })
        })?;

        tags.collect::<Result<Vec<_>>>()
    }

    pub fn get_tag_by_name(&self, name: &str) -> Result<Option<Tag>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, color, created_at FROM tags WHERE name = ?1")?;

        let mut rows = stmt.query([name])?;
        if let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let name: String = row.get(1)?;
            let color: Option<String> = row.get(2)?;
            let created_at_str: String = row.get(3)?;

            Ok(Some(Tag {
                id,
                name,
                color,
                created_at: DateTime::parse_from_rfc3339(&created_at_str)
                    .unwrap_or_else(|_| chrono::Local::now().into())
                    .with_timezone(&Utc),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn update_tag(&self, id: i64, name: Option<&str>, color: Option<&str>) -> Result<()> {
        if let Some(name) = name {
            self.conn.execute(
                "UPDATE tags SET name = ?1 WHERE id = ?2",
                [name, &id.to_string()],
            )?;
        }
        if let Some(color) = color {
            self.conn.execute(
                "UPDATE tags SET color = ?1 WHERE id = ?2",
                [color, &id.to_string()],
            )?;
        }
        Ok(())
    }

    pub fn delete_tag(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM tags WHERE id = ?1", [&id.to_string()])?;
        Ok(())
    }

    pub fn get_record_tags(&self, record_id: Uuid) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.name FROM tags t
             JOIN record_tags rt ON t.id = rt.tag_id
             WHERE rt.record_id = ?1
             ORDER BY t.name",
        )?;

        let tags = stmt.query_map([&record_id.to_string()], |row| {
            let name: String = row.get(0)?;
            Ok(name)
        })?;

        tags.collect::<Result<Vec<_>>>()
    }

    pub fn create_person(&self, name: &str) -> Result<Person> {
        let now = Utc::now();
        self.conn.execute(
            "INSERT INTO persons (name, created_at) VALUES (?1, ?2)",
            [name, &now.to_rfc3339()],
        )?;

        let id = self.conn.last_insert_rowid();
        Ok(Person {
            id,
            name: name.to_string(),
            created_at: now,
        })
    }

    pub fn get_persons(&self) -> Result<Vec<Person>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, created_at FROM persons ORDER BY name")?;

        let persons = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let name: String = row.get(1)?;
            let created_at_str: String = row.get(2)?;

            Ok(Person {
                id,
                name,
                created_at: DateTime::parse_from_rfc3339(&created_at_str)
                    .unwrap_or_else(|_| chrono::Local::now().into())
                    .with_timezone(&Utc),
            })
        })?;

        persons.collect::<Result<Vec<_>>>()
    }

    pub fn get_person_by_name(&self, name: &str) -> Result<Option<Person>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, created_at FROM persons WHERE name = ?1")?;

        let mut rows = stmt.query([name])?;
        if let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let name: String = row.get(1)?;
            let created_at_str: String = row.get(3)?;

            Ok(Some(Person {
                id,
                name,
                created_at: DateTime::parse_from_rfc3339(&created_at_str)
                    .unwrap_or_else(|_| chrono::Local::now().into())
                    .with_timezone(&Utc),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn update_person(&self, id: i64, name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE persons SET name = ?1 WHERE id = ?2",
            [name, &id.to_string()],
        )?;
        Ok(())
    }

    pub fn delete_person(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM persons WHERE id = ?1", [&id.to_string()])?;
        Ok(())
    }

    pub fn get_record_persons(&self, record_id: Uuid) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.name FROM persons p
             JOIN record_persons rp ON p.id = rp.person_id
             WHERE rp.record_id = ?1
             ORDER BY p.name",
        )?;

        let persons = stmt.query_map([&record_id.to_string()], |row| {
            let name: String = row.get(0)?;
            Ok(name)
        })?;

        persons.collect::<Result<Vec<_>>>()
    }

    pub fn create_attachment(&self, attachment: &Attachment) -> Result<()> {
        self.conn.execute(
            "INSERT INTO attachments (id, record_id, file_name, file_path, file_size, mime_type, width, height, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                file_name = excluded.file_name,
                file_path = excluded.file_path,
                file_size = excluded.file_size,
                mime_type = excluded.mime_type,
                width = excluded.width,
                height = excluded.height",
            [
                &attachment.id as &dyn rusqlite::ToSql,
                &attachment.record_id as &dyn rusqlite::ToSql,
                &attachment.file_name as &dyn rusqlite::ToSql,
                &attachment.file_path as &dyn rusqlite::ToSql,
                &(attachment.file_size as i64) as &dyn rusqlite::ToSql,
                &attachment.mime_type as &dyn rusqlite::ToSql,
                &(attachment.width as i64) as &dyn rusqlite::ToSql,
                &(attachment.height as i64) as &dyn rusqlite::ToSql,
                &attachment.created_at.to_rfc3339() as &dyn rusqlite::ToSql,
            ],
        )?;
        Ok(())
    }

    pub fn get_attachments(&self, record_id: Uuid) -> Result<Vec<Attachment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, record_id, file_name, file_path, file_size, mime_type, width, height, created_at
             FROM attachments
             WHERE record_id = ?1
             ORDER BY created_at DESC"
        )?;

        let attachments = stmt.query_map([&record_id.to_string()], |row| {
            let id: String = row.get(0)?;
            let record_id: String = row.get(1)?;
            let file_name: String = row.get(2)?;
            let file_path: String = row.get(3)?;
            let file_size: i64 = row.get(4)?;
            let mime_type: String = row.get(5)?;
            let width: i64 = row.get(6)?;
            let height: i64 = row.get(7)?;
            let created_at_str: String = row.get(8)?;

            Ok(Attachment {
                id,
                record_id,
                file_name,
                file_path,
                file_size: file_size as usize,
                mime_type,
                width: width as u32,
                height: height as u32,
                created_at: DateTime::parse_from_rfc3339(&created_at_str)
                    .unwrap_or_else(|_| chrono::Local::now().into())
                    .with_timezone(&Utc),
            })
        })?;

        attachments.collect::<Result<Vec<_>>>()
    }

    pub fn delete_attachment(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM attachments WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn add_tag_to_record(&self, record_id: Uuid, tag_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO record_tags (record_id, tag_id) VALUES (?1, ?2)",
            [record_id.to_string(), tag_id.to_string()],
        )?;
        Ok(())
    }

    pub fn add_person_to_record(&self, record_id: Uuid, person_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO record_persons (record_id, person_id) VALUES (?1, ?2)",
            [record_id.to_string(), person_id.to_string()],
        )?;
        Ok(())
    }

    pub fn get_record_by_id(&self, id: Uuid) -> Result<Option<Record>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, content, priority, status, created_at, updated_at, completed_at, scheduled_for, due_date, notified_at, cancelled_reason, record_type
             FROM records WHERE id = ?1"
        )?;

        let mut rows = stmt.query([id.to_string()])?;
        if let Some(row) = rows.next()? {
            let mut record = self.row_to_record(row)?;
            self.load_record_associations(&mut record)?;
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }

    fn row_to_record(&self, row: &rusqlite::Row) -> Result<Record> {
        let id_str: String = row.get(0)?;
        let title: Option<String> = row.get(1)?;
        let content: String = row.get(2)?;
        let priority_int: Option<i64> = row.get(3)?;
        let status_str: Option<String> = row.get(4)?;
        let created_at_str: String = row.get(5)?;
        let updated_at_str: String = row.get(6)?;
        let completed_at_str: Option<String> = row.get(7)?;
        let scheduled_for_str: Option<String> = row.get(8)?;
        let due_date_str: Option<String> = row.get(9)?;
        let notified_at_str: Option<String> = row.get(10)?;
        let cancelled_reason: Option<String> = row.get(11)?;
        let record_type_str: String = row.get(12)?;

        eprintln!(
            "[DB] Row: id={}, title={:?}, content='{}', priority_int={:?}",
            id_str, title, content, priority_int
        );

        Ok(Record {
            id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
            title,
            content,
            priority: priority_int.map(|p| match p {
                0 => Priority::High,
                1 => Priority::Medium,
                _ => Priority::Low,
            }),
            status: status_str.and_then(|s| match s.as_str() {
                "todo" => Some(TaskStatus::Todo),
                "in_progress" => Some(TaskStatus::InProgress),
                "done" => Some(TaskStatus::Done),
                "cancelled" => Some(TaskStatus::Cancelled),
                _ => None,
            }),
            created_at: DateTime::parse_from_rfc3339(&created_at_str)
                .unwrap_or_else(|_| chrono::Local::now().into())
                .with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                .unwrap_or_else(|_| chrono::Local::now().into())
                .with_timezone(&Utc),
            completed_at: completed_at_str.filter(|s| !s.is_empty()).and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|d| d.with_timezone(&Utc))
            }),
            scheduled_for: scheduled_for_str.filter(|s| !s.is_empty()).and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|d| d.with_timezone(&Utc))
            }),
            due_date: due_date_str.filter(|s| !s.is_empty()).and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|d| d.with_timezone(&Utc))
            }),
            notified_at: notified_at_str.filter(|s| !s.is_empty()).and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|d| d.with_timezone(&Utc))
            }),
            cancelled_reason,
            record_type: match record_type_str.as_str() {
                "note" => RecordType::Note,
                "event" => RecordType::Event,
                "idea" => RecordType::Idea,
                _ => RecordType::Task,
            },
            tags: Vec::new(),
            persons: Vec::new(),
        })
    }

    fn load_record_associations(&self, record: &mut Record) -> Result<()> {
        record.tags = self.get_record_tags(record.id)?;
        record.persons = self.get_record_persons(record.id)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_db() -> (Database, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Database::new(&db_path).unwrap();
        (db, temp_dir)
    }

    #[test]
    fn test_create_and_get_record() {
        let (db, _temp) = setup_test_db();

        let record = Record::new_task(
            "Test Title".to_string(),
            "Test task".to_string(),
            Priority::High,
        );
        db.create_record(&record).unwrap();

        let tasks = db.get_tasks(false).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, Some("Test Title".to_string()));
        assert_eq!(tasks[0].content, "Test task");
    }

    #[test]
    fn test_tags_crud() {
        let (db, _temp) = setup_test_db();

        let tag = db.create_tag("important", Some("#ff0000")).unwrap();
        assert_eq!(tag.name, "important");
        assert_eq!(tag.color, Some("#ff0000".to_string()));

        let tags = db.get_tags().unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "important");

        let found = db.get_tag_by_name("important").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "important");

        db.update_tag(tag.id, Some("critical"), None).unwrap();
        let updated = db.get_tag_by_name("critical").unwrap();
        assert!(updated.is_some());

        db.delete_tag(tag.id).unwrap();
        let tags = db.get_tags().unwrap();
        assert_eq!(tags.len(), 0);
    }

    #[test]
    fn test_persons_crud() {
        let (db, _temp) = setup_test_db();

        let person = db.create_person("Alice").unwrap();
        assert_eq!(person.name, "Alice");

        let persons = db.get_persons().unwrap();
        assert_eq!(persons.len(), 1);
        assert_eq!(persons[0].name, "Alice");

        db.update_person(person.id, "Bob").unwrap();
        let persons = db.get_persons().unwrap();
        assert_eq!(persons[0].name, "Bob");

        db.delete_person(person.id).unwrap();
        let persons = db.get_persons().unwrap();
        assert_eq!(persons.len(), 0);
    }

    #[test]
    fn test_attachments_crud() {
        let (db, _temp) = setup_test_db();

        let record = Record::new_note("Test note".to_string());
        db.create_record(&record).unwrap();

        let attachment = Attachment {
            id: Uuid::new_v4().to_string(),
            record_id: record.id.to_string(),
            file_name: "test.png".to_string(),
            file_path: "/path/to/test.png".to_string(),
            file_size: 1024,
            mime_type: "image/png".to_string(),
            width: 100,
            height: 100,
            created_at: Utc::now(),
        };

        db.create_attachment(&attachment).unwrap();

        let attachments = db.get_attachments(record.id).unwrap();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].file_name, "test.png");

        db.delete_attachment(&attachment.id).unwrap();
        let attachments = db.get_attachments(record.id).unwrap();
        assert_eq!(attachments.len(), 0);
    }

    #[test]
    fn test_record_with_tags_and_persons() {
        let (db, _temp) = setup_test_db();

        let mut record = Record::new_note("Test note with tags".to_string());
        record.tags = vec!["tag1".to_string(), "tag2".to_string()];
        record.persons = vec!["Alice".to_string()];

        db.create_record(&record).unwrap();

        let notes = db.get_notes().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].tags.len(), 2);
        assert!(notes[0].tags.contains(&"tag1".to_string()));
        assert!(notes[0].tags.contains(&"tag2".to_string()));
        assert_eq!(notes[0].persons.len(), 1);
        assert_eq!(notes[0].persons[0], "Alice");
    }

    #[test]
    fn test_search_records() {
        let (db, _temp) = setup_test_db();

        let record1 = Record::new_note("First note about cats".to_string());
        db.create_record(&record1).unwrap();

        let record2 = Record::new_note("Second note about dogs".to_string());
        db.create_record(&record2).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(100));

        let results = db.search_records("cats").unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("cats"));
    }

    #[test]
    fn test_migrate_v3_to_v4_rebuilds_search_index_from_title_and_content() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("migration-v3.db");
        let record_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();

        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "CREATE TABLE schema_version (
                    version INTEGER PRIMARY KEY,
                    updated_at TEXT NOT NULL
                )",
                [],
            )
            .unwrap();
            conn.execute(
                "CREATE TABLE records (
                    id TEXT PRIMARY KEY,
                    title TEXT,
                    content TEXT NOT NULL,
                    priority INTEGER,
                    status TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    completed_at TEXT,
                    scheduled_for TEXT,
                    due_date TEXT,
                    notified_at TEXT,
                    cancelled_reason TEXT,
                    record_type TEXT NOT NULL
                )",
                [],
            )
            .unwrap();
            conn.execute(
                "CREATE VIRTUAL TABLE records_fts USING fts5(record_id, content)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO records (
                    id, title, content, priority, status, created_at, updated_at,
                    completed_at, scheduled_for, due_date, notified_at, cancelled_reason, record_type
                ) VALUES (?1, ?2, ?3, NULL, 'todo', ?4, ?4, NULL, NULL, NULL, NULL, NULL, 'task')",
                [
                    &record_id.to_string(),
                    &"试试看".to_string(),
                    &"正文保留旧值".to_string(),
                    &now,
                ],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO records_fts (record_id, content) VALUES (?1, ?2)",
                [&record_id.to_string(), &"正文保留旧值".to_string()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO schema_version (version, updated_at) VALUES (3, ?1)",
                [&now],
            )
            .unwrap();
        }

        let db = Database::new(&db_path).unwrap();

        let title_results = db.search_records("试试").unwrap();
        assert_eq!(title_results.len(), 1);
        assert_eq!(title_results[0].title.as_deref(), Some("试试看"));

        let body_results = db.search_records("旧值").unwrap();
        assert_eq!(body_results.len(), 1);

        let indexed_content: String = db
            .conn
            .query_row(
                "SELECT content FROM records_fts WHERE record_id = ?1",
                [record_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert!(indexed_content.contains("试试看"));
        assert!(indexed_content.contains("正文保留旧值"));
    }

    #[test]
    fn test_timeline() {
        let (db, _temp) = setup_test_db();

        for i in 0..5 {
            let record = Record::new_note(format!("Note {}", i));
            db.create_record(&record).unwrap();
        }

        let timeline = db.get_timeline(3, 0).unwrap();
        assert_eq!(timeline.len(), 3);

        let timeline2 = db.get_timeline(3, 3).unwrap();
        assert_eq!(timeline2.len(), 2);
    }
}
