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
                record_type TEXT NOT NULL
            )",
            [],
        )?;
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

        self.conn.execute(
            "INSERT INTO records (id, content, priority, created_at, completed_at, record_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                content = excluded.content,
                priority = excluded.priority,
                completed_at = excluded.completed_at",
            [
                record.id.to_string(),
                record.content.clone(),
                priority_val.map(|v| v.to_string()).unwrap_or_default(),
                record.created_at.to_rfc3339(),
                record.completed_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
                record_type_str.to_string(),
            ],
        )?;
        Ok(())
    }

    pub fn get_tasks(&self, _completed: bool) -> Result<Vec<Record>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, content, priority, created_at, completed_at, record_type
             FROM records
             WHERE record_type = 'task'
             ORDER BY created_at DESC"
        )?;

        let records = stmt.query_map([], |row| {
            let id_str: String = row.get(0)?;
            let priority_int: Option<i64> = row.get(2)?;
            let created_at_str: String = row.get(3)?;
            let completed_at_str: Option<String> = row.get(4)?;
            let record_type_str: String = row.get(5)?;

            Ok(Record {
                id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
                content: row.get(1)?,
                priority: priority_int.map(|p| match p {
                    0 => Priority::High,
                    1 => Priority::Medium,
                    _ => Priority::Low,
                }),
                created_at: DateTime::parse_from_rfc3339(&created_at_str)
                    .unwrap_or_else(|_| chrono::Local::now().into())
                    .with_timezone(&Utc),
                completed_at: completed_at_str.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))
                }),
                record_type: match record_type_str.as_str() {
                    "note" => RecordType::Note,
                    "event" => RecordType::Event,
                    _ => RecordType::Task,
                },
            })
        })?;

        records.collect()
    }
}
