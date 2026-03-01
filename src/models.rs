use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Priority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub id: Uuid,
    pub content: String,
    pub priority: Option<Priority>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub record_type: RecordType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecordType {
    Task,
    Note,
    Event,
}

impl Record {
    pub fn new_task(content: String, priority: Priority) -> Self {
        Self {
            id: Uuid::new_v4(),
            content,
            priority: Some(priority),
            created_at: Utc::now(),
            completed_at: None,
            record_type: RecordType::Task,
        }
    }

    pub fn complete(&mut self) {
        self.completed_at = Some(Utc::now());
    }

    pub fn is_completed(&self) -> bool {
        self.completed_at.is_some()
    }
}
