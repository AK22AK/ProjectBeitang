use crate::models::{Priority, Record, RecordType};
use anyhow::Result;
use chrono::{DateTime, Local};
use rusqlite::{params, Connection, OptionalExtension};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Priority, RecordType};

    #[test]
    fn test_create_and_get_record() {
        let db = Database::open_in_memory().unwrap();
        let record = Record::new_task("测试任务", Priority::High);

        db.insert_record(&record).unwrap();
        let retrieved = db.get_record(record.id).unwrap().unwrap();

        assert_eq!(retrieved.content, "测试任务");
        assert_eq!(retrieved.record_type, RecordType::Task);
        assert_eq!(retrieved.priority, Some(Priority::High));
    }

    #[test]
    fn test_get_tasks_filters_by_type() {
        let db = Database::open_in_memory().unwrap();

        let task = Record::new_task("一个任务", Priority::Medium);
        let idea = Record::new_idea("一个想法");

        db.insert_record(&task).unwrap();
        db.insert_record(&idea).unwrap();

        let tasks = db.get_tasks(true).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].content, "一个任务");
    }

    #[test]
    fn test_complete_task() {
        let db = Database::open_in_memory().unwrap();
        let mut task = Record::new_task("待完成任务", Priority::Low);

        db.insert_record(&task).unwrap();
        assert!(!task.is_completed());

        task.complete();
        db.update_record(&task).unwrap();

        let retrieved = db.get_record(task.id).unwrap().unwrap();
        assert!(retrieved.is_completed());
    }
}
