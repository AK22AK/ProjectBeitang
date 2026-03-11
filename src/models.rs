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
    pub scheduled_for: Option<DateTime<Utc>>,
    pub due_date: Option<DateTime<Utc>>,
    pub record_type: RecordType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
            scheduled_for: None,
            due_date: None,
            record_type: RecordType::Task,
        }
    }

    pub fn new_note(content: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            content,
            priority: None,
            created_at: Utc::now(),
            completed_at: None,
            scheduled_for: None,
            due_date: None,
            record_type: RecordType::Note,
        }
    }

    pub fn complete(&mut self) {
        self.completed_at = Some(Utc::now());
    }

    pub fn is_completed(&self) -> bool {
        self.completed_at.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_task_creates_task_with_correct_properties() {
        let task = Record::new_task("Test content".to_string(), Priority::High);

        assert_eq!(task.content, "Test content");
        assert_eq!(task.priority, Some(Priority::High));
        assert_eq!(task.record_type, RecordType::Task);
        assert!(!task.is_completed());
        assert!(task.scheduled_for.is_none());
        assert!(task.due_date.is_none());
    }

    #[test]
    fn test_complete_marks_task_as_completed() {
        let mut task = Record::new_task("Test".to_string(), Priority::Medium);
        assert!(!task.is_completed());

        task.complete();

        assert!(task.is_completed());
        assert!(task.completed_at.is_some());
    }

    #[test]
    fn test_priority_equality() {
        assert_eq!(Priority::High, Priority::High);
        assert_ne!(Priority::High, Priority::Medium);
    }

    #[test]
    fn test_new_note_creates_note_with_correct_properties() {
        let note = Record::new_note("Test note content".to_string());

        assert_eq!(note.content, "Test note content");
        assert_eq!(note.priority, None);
        assert_eq!(note.record_type, RecordType::Note);
        assert!(!note.is_completed());
        assert!(note.scheduled_for.is_none());
        assert!(note.due_date.is_none());
    }

    #[test]
    fn test_note_can_be_completed() {
        let mut note = Record::new_note("Test note".to_string());
        assert!(!note.is_completed());

        note.complete();

        assert!(note.is_completed());
        assert!(note.completed_at.is_some());
    }
}
