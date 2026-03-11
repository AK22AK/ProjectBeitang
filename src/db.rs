use crate::models::{Priority, Record, RecordType};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, Result};
use uuid::Uuid;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init_tables()?;
        Ok(db)
    }

    fn init_tables(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS records (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                priority INTEGER,
                created_at TEXT NOT NULL,
                completed_at TEXT,
                scheduled_for TEXT,
                due_date TEXT,
                record_type TEXT NOT NULL
            )",
            [],
        )?;
        // Graceful migrations for existing DBs
        let _ = self.conn.execute("ALTER TABLE records ADD COLUMN scheduled_for TEXT", []);
        let _ = self.conn.execute("ALTER TABLE records ADD COLUMN due_date TEXT", []);
        Ok(())
    }

    pub fn create_record(&self, record: &Record) -> Result<()> {
        let priority_val = record.priority.as_ref().map(|p| match p {
            Priority::High => 0i64,
            Priority::Medium => 1i64,
            Priority::Low => 2i64,
        });

        let record_type_str = match record.record_type {
            RecordType::Task => "task",
            RecordType::Note => "note",
            RecordType::Event => "event",
        };

        eprintln!("[DB] create_record: id={}, content='{}', priority={:?}",
                  record.id, record.content, priority_val);

        self.conn.execute(
            "INSERT INTO records (id, content, priority, created_at, completed_at, scheduled_for, due_date, record_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                content = excluded.content,
                priority = excluded.priority,
                completed_at = excluded.completed_at,
                scheduled_for = excluded.scheduled_for,
                due_date = excluded.due_date",
            [
                &record.id.to_string() as &dyn rusqlite::ToSql,
                &record.content as &dyn rusqlite::ToSql,
                &priority_val as &dyn rusqlite::ToSql,  // 直接传递 Option<i64>，None 会成为 NULL
                &record.created_at.to_rfc3339() as &dyn rusqlite::ToSql,
                &record.completed_at.map(|t| t.to_rfc3339()) as &dyn rusqlite::ToSql,
                &record.scheduled_for.map(|t| t.to_rfc3339()) as &dyn rusqlite::ToSql,
                &record.due_date.map(|t| t.to_rfc3339()) as &dyn rusqlite::ToSql,
                &record_type_str as &dyn rusqlite::ToSql,
            ],
        )?;
        eprintln!("[DB] create_record succeeded");
        Ok(())
    }

    pub fn get_tasks(&self, _completed: bool) -> Result<Vec<Record>> {
        eprintln!("[DB] get_tasks called");
        let mut stmt = self.conn.prepare(
            "SELECT id, content, priority, created_at, completed_at, scheduled_for, due_date, record_type
             FROM records
             WHERE record_type = 'task'
             ORDER BY created_at DESC"
        )?;

        let records = stmt.query_map([], |row| {
            let id_str: String = row.get(0)?;
            let content: String = row.get(1)?;
            // priority 存储为 INTEGER，直接读取为 i64
            let priority_int: Option<i64> = row.get(2)?;
            let created_at_str: String = row.get(3)?;
            let completed_at_str: Option<String> = row.get(4)?;
            let scheduled_for_str: Option<String> = row.get(5)?;
            let due_date_str: Option<String> = row.get(6)?;
            let record_type_str: String = row.get(7)?;

            eprintln!("[DB] Row: id={}, content='{}', priority_int={:?}", id_str, content, priority_int);

            Ok(Record {
                id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
                content,
                priority: priority_int.map(|p| match p {
                    0 => Priority::High,
                    1 => Priority::Medium,
                    _ => Priority::Low,
                }),
                created_at: DateTime::parse_from_rfc3339(&created_at_str)
                    .unwrap_or_else(|_| chrono::Local::now().into())
                    .with_timezone(&Utc),
                completed_at: completed_at_str.filter(|s| !s.is_empty()).and_then(|s| {
                    DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))
                }),
                scheduled_for: scheduled_for_str.filter(|s| !s.is_empty()).and_then(|s| {
                    DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))
                }),
                due_date: due_date_str.filter(|s| !s.is_empty()).and_then(|s| {
                    DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))
                }),
                record_type: match record_type_str.as_str() {
                    "note" => RecordType::Note,
                    "event" => RecordType::Event,
                    _ => RecordType::Task,
                },
            })
        })?;

        let result: Vec<Record> = records.collect::<Result<Vec<_>>>()?;
        eprintln!("[DB] get_tasks returning {} records", result.len());
        Ok(result)
    }

    pub fn get_notes(&self) -> Result<Vec<Record>> {
        eprintln!("[DB] get_notes called");
        let mut stmt = self.conn.prepare(
            "SELECT id, content, priority, created_at, completed_at, scheduled_for, due_date, record_type
             FROM records
             WHERE record_type = 'note'
             ORDER BY created_at DESC"
        )?;

        let records = stmt.query_map([], |row| {
            let id_str: String = row.get(0)?;
            let content: String = row.get(1)?;
            let priority_int: Option<i64> = row.get(2)?;
            let created_at_str: String = row.get(3)?;
            let completed_at_str: Option<String> = row.get(4)?;
            let scheduled_for_str: Option<String> = row.get(5)?;
            let due_date_str: Option<String> = row.get(6)?;
            let record_type_str: String = row.get(7)?;

            eprintln!("[DB] Row: id={}, content='{}', record_type='{}'", id_str, content, record_type_str);

            Ok(Record {
                id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
                content,
                priority: priority_int.map(|p| match p {
                    0 => Priority::High,
                    1 => Priority::Medium,
                    _ => Priority::Low,
                }),
                created_at: DateTime::parse_from_rfc3339(&created_at_str)
                    .unwrap_or_else(|_| chrono::Local::now().into())
                    .with_timezone(&Utc),
                completed_at: completed_at_str.filter(|s| !s.is_empty()).and_then(|s| {
                    DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))
                }),
                scheduled_for: scheduled_for_str.filter(|s| !s.is_empty()).and_then(|s| {
                    DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))
                }),
                due_date: due_date_str.filter(|s| !s.is_empty()).and_then(|s| {
                    DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))
                }),
                record_type: match record_type_str.as_str() {
                    "note" => RecordType::Note,
                    "event" => RecordType::Event,
                    _ => RecordType::Task,
                },
            })
        })?;

        let result: Vec<Record> = records.collect::<Result<Vec<_>>>()?;
        eprintln!("[DB] get_notes returning {} records", result.len());
        Ok(result)
    }

    pub fn delete_record(&self, id: Uuid) -> Result<()> {
        eprintln!("[DB] delete_record called for id: {}", id);
        self.conn.execute(
            "DELETE FROM records WHERE id = ?1",
            [&id.to_string()],
        )?;
        eprintln!("[DB] delete_record succeeded");
        Ok(())
    }
}
