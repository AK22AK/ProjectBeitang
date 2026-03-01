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
