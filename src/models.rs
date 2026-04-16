use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Priority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Todo,
    InProgress,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub id: Uuid,
    /// 标题 - 任务必填，记录可选
    pub title: Option<String>,
    /// 内容/详情 - 任务的详细描述或记录的内容
    pub content: String,
    pub priority: Option<Priority>,
    pub status: Option<TaskStatus>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub due_date: Option<DateTime<Utc>>,
    pub notified_at: Option<DateTime<Utc>>,
    pub cancelled_reason: Option<String>,
    pub record_type: RecordType,
    #[serde(skip)]
    pub tags: Vec<String>,
    #[serde(skip)]
    pub persons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecordType {
    Task,
    Note,
    Event,
    Idea,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    pub id: i64,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetadataCatalogEntry {
    pub name: String,
    pub usage_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelineQuery {
    pub limit: usize,
    pub offset: usize,
    pub tags: Vec<String>,
    pub persons: Vec<String>,
}

impl TimelineQuery {
    pub fn new(limit: usize, offset: usize) -> Self {
        Self {
            limit,
            offset,
            tags: Vec::new(),
            persons: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String, // UUID
    pub record_id: String,
    pub file_name: String,
    pub file_path: String,
    pub file_size: usize,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    pub created_at: DateTime<Utc>,
    pub status: AttachmentStatus,
    pub error_message: Option<String>,
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AttachmentStatus {
    Processing,
    Ready,
    Failed,
}

impl AttachmentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Processing => "processing",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "processing" => Self::Processing,
            "failed" => Self::Failed,
            _ => Self::Ready,
        }
    }
}

impl Record {
    /// 创建新任务 - title 作为任务标题，content 作为详细描述（可选）
    pub fn new_task(title: String, content: String, priority: Priority) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: Some(title),
            content,
            priority: Some(priority),
            status: Some(TaskStatus::Todo),
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            scheduled_for: None,
            due_date: None,
            notified_at: None,
            cancelled_reason: None,
            record_type: RecordType::Task,
            tags: Vec::new(),
            persons: Vec::new(),
        }
    }

    /// 创建新记录/笔记 - content 作为主要内容，title 可选（可从第一行提取）
    pub fn new_note(content: String) -> Self {
        let now = Utc::now();
        // 从内容中提取第一行作为标题（如果有）
        let title = Self::extract_title_from_content(&content);
        Self {
            id: Uuid::new_v4(),
            title,
            content: content.clone(),
            priority: None,
            status: None,
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            scheduled_for: None,
            due_date: None,
            notified_at: None,
            cancelled_reason: None,
            record_type: RecordType::Note,
            tags: Vec::new(),
            persons: Vec::new(),
        }
    }

    /// 创建新记录/笔记 - 显式指定标题与正文
    pub fn new_note_with_title(title: Option<String>, content: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title,
            content,
            priority: None,
            status: None,
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            scheduled_for: None,
            due_date: None,
            notified_at: None,
            cancelled_reason: None,
            record_type: RecordType::Note,
            tags: Vec::new(),
            persons: Vec::new(),
        }
    }

    /// 创建新想法 - 类似笔记，content 为主
    pub fn new_idea(content: String) -> Self {
        let now = Utc::now();
        let title = Self::extract_title_from_content(&content);
        Self {
            id: Uuid::new_v4(),
            title,
            content: content.clone(),
            priority: None,
            status: None,
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            scheduled_for: None,
            due_date: None,
            notified_at: None,
            cancelled_reason: None,
            record_type: RecordType::Idea,
            tags: Vec::new(),
            persons: Vec::new(),
        }
    }

    /// 从内容中提取第一行非空文本作为标题
    fn extract_title_from_content(content: &str) -> Option<String> {
        content
            .lines()
            .find(|line| !line.trim().is_empty())
            .map(|line| line.trim().to_string())
    }

    /// 获取用于列表显示的标题
    /// - 任务：返回 title（必填）
    /// - 记录/笔记：优先返回 title，否则返回 content 前50字符
    pub fn display_title(&self) -> String {
        match self.record_type {
            RecordType::Task => self
                .title
                .clone()
                .unwrap_or_else(|| "无标题任务".to_string()),
            _ => self
                .title
                .clone()
                .or_else(|| {
                    let preview: String = self.content.chars().take(50).collect();
                    if preview.is_empty() {
                        None
                    } else {
                        Some(preview)
                    }
                })
                .unwrap_or_else(|| "无标题".to_string()),
        }
    }

    /// 获取内容预览（用于列表显示）
    pub fn content_preview(&self, max_len: usize) -> String {
        self.content.chars().take(max_len).collect()
    }

    pub fn sync_task_lifecycle_fields(
        &mut self,
        previous_status: Option<TaskStatus>,
        now: DateTime<Utc>,
    ) {
        if self.record_type != RecordType::Task {
            return;
        }

        match self.status {
            Some(TaskStatus::InProgress) => {
                if previous_status != Some(TaskStatus::InProgress) || self.started_at.is_none() {
                    self.started_at = Some(now);
                }
                self.completed_at = None;
                self.cancelled_reason = None;
            }
            Some(TaskStatus::Todo) => {
                self.completed_at = None;
                self.cancelled_reason = None;
            }
            Some(TaskStatus::Done) | Some(TaskStatus::Cancelled) => {
                if self.completed_at.is_none() {
                    self.completed_at = Some(now);
                }
            }
            None => {}
        }
    }

    pub fn complete(&mut self) {
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        self.status = Some(TaskStatus::Done);
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
        let task = Record::new_task(
            "Test Title".to_string(),
            "Test content details".to_string(),
            Priority::High,
        );

        assert_eq!(task.title, Some("Test Title".to_string()));
        assert_eq!(task.content, "Test content details");
        assert_eq!(task.priority, Some(Priority::High));
        assert_eq!(task.record_type, RecordType::Task);
        assert!(!task.is_completed());
        assert!(task.scheduled_for.is_none());
        assert!(task.due_date.is_none());
        assert!(task.notified_at.is_none());
    }

    #[test]
    fn test_complete_marks_task_as_completed() {
        let mut task = Record::new_task("Test".to_string(), "".to_string(), Priority::Medium);
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
        assert!(note.notified_at.is_none());
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
