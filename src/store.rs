use crate::db::Database;
use crate::models::{Person, Record, Tag};
use async_channel::{unbounded, Receiver, Sender};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct DashboardData {
    pub in_progress: Vec<Record>,     // 进行中任务
    pub pending_tasks: Vec<Record>,   // 待办任务（按四象限排序）
    pub recent_records: Vec<Record>,  // 最近记录（用于回顾）
    pub total_pending: usize,         // 待办总数
    pub total_in_progress: usize,     // 进行中总数
    pub total_completed_today: usize, // 今日完成数
}

#[derive(Clone)]
pub struct Store {
    sender: Sender<StoreCommand>,
}

pub enum StoreCommand {
    GetTasks {
        completed: bool,
        respond_to: Sender<Result<Vec<Record>, String>>,
    },
    GetNotes {
        respond_to: Sender<Result<Vec<Record>, String>>,
    },
    CreateRecord {
        record: Record,
        respond_to: Sender<Result<(), String>>,
    },
    UpdateRecord {
        record: Record,
        respond_to: Sender<Result<(), String>>,
    },
    DeleteRecord {
        id: uuid::Uuid,
        respond_to: Sender<Result<(), String>>,
    },
    // 时间线查询
    GetTimeline {
        limit: usize,
        offset: usize,
        respond_to: Sender<Result<Vec<Record>, String>>,
    },
    // 全文搜索
    SearchRecords {
        query: String,
        respond_to: Sender<Result<Vec<Record>, String>>,
    },
    GetRecordsByTag {
        tag: String,
        respond_to: Sender<Result<Vec<Record>, String>>,
    },
    GetRecordsByPerson {
        person: String,
        respond_to: Sender<Result<Vec<Record>, String>>,
    },
    // 看板数据
    GetDashboard {
        respond_to: Sender<Result<DashboardData, String>>,
    },
    // 任务状态变更
    StartTask {
        id: uuid::Uuid,
        respond_to: Sender<Result<(), String>>,
    },
    CompleteTask {
        id: uuid::Uuid,
        respond_to: Sender<Result<(), String>>,
    },
    CancelTask {
        id: uuid::Uuid,
        reason: Option<String>,
        respond_to: Sender<Result<(), String>>,
    },
    // 标签操作
    GetAllTags {
        respond_to: Sender<Result<Vec<Tag>, String>>,
    },
    CreateTag {
        name: String,
        respond_to: Sender<Result<i64, String>>,
    },
    AddTagToRecord {
        record_id: uuid::Uuid,
        tag_id: i64,
        respond_to: Sender<Result<(), String>>,
    },
    // 人物操作
    GetAllPersons {
        respond_to: Sender<Result<Vec<Person>, String>>,
    },
    CreatePerson {
        name: String,
        respond_to: Sender<Result<i64, String>>,
    },
    AddPersonToRecord {
        record_id: uuid::Uuid,
        person_id: i64,
        respond_to: Sender<Result<(), String>>,
    },
}

pub struct StoreRuntime {
    receiver: Receiver<StoreCommand>,
    db: Option<Database>,
}

impl StoreRuntime {
    pub fn new(receiver: Receiver<StoreCommand>) -> Self {
        Self { receiver, db: None }
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

        // Start background reminder loop
        let db_path_clone = db_path.clone();
        if let Some(db_clone) = Database::new(&db_path_clone).ok() {
            eprintln!("[Store] Background reminder thread started");
            std::thread::spawn(move || loop {
                match db_clone.get_pending_reminders() {
                    Ok(records) => {
                        if !records.is_empty() {
                            eprintln!("[Store] Found {} pending reminders", records.len());
                        }
                        for mut record in records {
                            eprintln!("[Store] Processing reminder for task: {}", record.id);
                            match crate::notifier::Notifier::send_reminder(&record) {
                                Ok(_) => {
                                    eprintln!("[Store] Notification sent successfully");
                                    record.notified_at = Some(chrono::Utc::now());
                                    if let Err(e) = db_clone.create_record(&record) {
                                        eprintln!("[Store] Failed to update notified_at: {}", e);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("[Store] Failed to send notification: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[Store] get_pending_reminders failed: {}", e);
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(10));
            });
        } else {
            eprintln!("[Store] Failed to open background DB connection");
        }

        // 处理命令循环
        while let Ok(cmd) = self.receiver.recv().await {
            match cmd {
                StoreCommand::GetTasks {
                    completed,
                    respond_to,
                } => {
                    let result = self.handle_get_tasks(completed).await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::GetNotes { respond_to } => {
                    let result = self.handle_get_notes().await;
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
                StoreCommand::DeleteRecord { id, respond_to } => {
                    let result = self.handle_delete_record(id).await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::GetTimeline {
                    limit,
                    offset,
                    respond_to,
                } => {
                    let result = self.handle_get_timeline(limit, offset).await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::SearchRecords { query, respond_to } => {
                    let result = self.handle_search_records(query).await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::GetRecordsByTag { tag, respond_to } => {
                    let result = self.handle_get_records_by_tag(tag).await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::GetRecordsByPerson { person, respond_to } => {
                    let result = self.handle_get_records_by_person(person).await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::GetDashboard { respond_to } => {
                    let result = self.handle_get_dashboard().await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::StartTask { id, respond_to } => {
                    let result = self.handle_start_task(id).await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::CompleteTask { id, respond_to } => {
                    let result = self.handle_complete_task(id).await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::CancelTask {
                    id,
                    reason,
                    respond_to,
                } => {
                    let result = self.handle_cancel_task(id, reason).await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::GetAllTags { respond_to } => {
                    let result = self.handle_get_all_tags().await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::CreateTag { name, respond_to } => {
                    let result = self.handle_create_tag(name).await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::AddTagToRecord {
                    record_id,
                    tag_id,
                    respond_to,
                } => {
                    let result = self.handle_add_tag_to_record(record_id, tag_id).await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::GetAllPersons { respond_to } => {
                    let result = self.handle_get_all_persons().await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::CreatePerson { name, respond_to } => {
                    let result = self.handle_create_person(name).await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::AddPersonToRecord {
                    record_id,
                    person_id,
                    respond_to,
                } => {
                    let result = self.handle_add_person_to_record(record_id, person_id).await;
                    let _ = respond_to.send(result).await;
                }
            }
        }
    }

    async fn handle_get_tasks(&self, _completed: bool) -> Result<Vec<Record>, String> {
        eprintln!("[Store] handle_get_tasks called");
        match &self.db {
            Some(db) => {
                eprintln!("[Store] Database exists, querying...");
                match db.get_tasks(false) {
                    Ok(tasks) => {
                        eprintln!("[Store] Found {} tasks", tasks.len());
                        Ok(tasks)
                    }
                    Err(e) => {
                        eprintln!("[Store] Query failed: {}", e);
                        Err(format!("Database error: {}", e))
                    }
                }
            }
            None => {
                eprintln!("[Store] Database not initialized!");
                Err("Database not initialized".to_string())
            }
        }
    }

    async fn handle_create_record(&self, record: Record) -> Result<(), String> {
        match &self.db {
            Some(db) => db
                .create_record(&record)
                .map_err(|e| format!("Database error: {}", e)),
            None => Err("Database not initialized".to_string()),
        }
    }

    async fn handle_update_record(&self, record: Record) -> Result<(), String> {
        match &self.db {
            Some(db) => db
                .create_record(&record)
                .map_err(|e| format!("Database error: {}", e)),
            None => Err("Database not initialized".to_string()),
        }
    }

    async fn handle_get_notes(&self) -> Result<Vec<Record>, String> {
        eprintln!("[Store] handle_get_notes called");
        match &self.db {
            Some(db) => {
                eprintln!("[Store] Database exists, querying notes...");
                match db.get_notes() {
                    Ok(notes) => {
                        eprintln!("[Store] Found {} notes", notes.len());
                        Ok(notes)
                    }
                    Err(e) => {
                        eprintln!("[Store] Query failed: {}", e);
                        Err(format!("Database error: {}", e))
                    }
                }
            }
            None => {
                eprintln!("[Store] Database not initialized!");
                Err("Database not initialized".to_string())
            }
        }
    }

    async fn handle_delete_record(&self, id: uuid::Uuid) -> Result<(), String> {
        eprintln!("[Store] handle_delete_record called for id: {}", id);
        match &self.db {
            Some(db) => db
                .delete_record(id)
                .map_err(|e| format!("Database error: {}", e)),
            None => Err("Database not initialized".to_string()),
        }
    }

    async fn handle_get_timeline(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Record>, String> {
        eprintln!(
            "[Store] handle_get_timeline called with limit={}, offset={}",
            limit, offset
        );
        match &self.db {
            Some(db) => match db.get_timeline(limit as i64, offset as i64) {
                Ok(records) => {
                    eprintln!("[Store] Found {} timeline records", records.len());
                    Ok(records)
                }
                Err(e) => {
                    eprintln!("[Store] Timeline query failed: {}", e);
                    Err(format!("Database error: {}", e))
                }
            },
            None => Err("Database not initialized".to_string()),
        }
    }

    async fn handle_search_records(&self, query: String) -> Result<Vec<Record>, String> {
        eprintln!(
            "[Store] handle_search_records called with query='{}'",
            query
        );
        match &self.db {
            Some(db) => match db.search_records(&query) {
                Ok(records) => {
                    eprintln!("[Store] Found {} search results", records.len());
                    Ok(records)
                }
                Err(e) => {
                    eprintln!("[Store] Search query failed: {}", e);
                    Err(format!("Database error: {}", e))
                }
            },
            None => Err("Database not initialized".to_string()),
        }
    }

    async fn handle_get_records_by_tag(&self, tag: String) -> Result<Vec<Record>, String> {
        eprintln!(
            "[Store] handle_get_records_by_tag called with tag='{}'",
            tag
        );
        match &self.db {
            Some(db) => match db.get_records_by_tag(&tag) {
                Ok(records) => {
                    eprintln!("[Store] Found {} tagged records", records.len());
                    Ok(records)
                }
                Err(e) => {
                    eprintln!("[Store] Tagged records query failed: {}", e);
                    Err(format!("Database error: {}", e))
                }
            },
            None => Err("Database not initialized".to_string()),
        }
    }

    async fn handle_get_records_by_person(&self, person: String) -> Result<Vec<Record>, String> {
        eprintln!(
            "[Store] handle_get_records_by_person called with person='{}'",
            person
        );
        match &self.db {
            Some(db) => match db.get_records_by_person(&person) {
                Ok(records) => {
                    eprintln!("[Store] Found {} person-linked records", records.len());
                    Ok(records)
                }
                Err(e) => {
                    eprintln!("[Store] Person records query failed: {}", e);
                    Err(format!("Database error: {}", e))
                }
            },
            None => Err("Database not initialized".to_string()),
        }
    }

    async fn handle_get_dashboard(&self) -> Result<DashboardData, String> {
        eprintln!("[Store] handle_get_dashboard called");
        match &self.db {
            Some(db) => {
                let tasks = match db.get_tasks(false) {
                    Ok(tasks) => tasks,
                    Err(e) => return Err(format!("Database error: {}", e)),
                };

                let in_progress: Vec<Record> = tasks
                    .iter()
                    .filter(|t| {
                        t.status
                            .as_ref()
                            .map(|s| matches!(s, crate::models::TaskStatus::InProgress))
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect();

                let pending_tasks: Vec<Record> = tasks
                    .iter()
                    .filter(|t| {
                        t.status
                            .as_ref()
                            .map(|s| matches!(s, crate::models::TaskStatus::Todo))
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect();

                let recent_records = match db.get_timeline(20, 0) {
                    Ok(records) => records,
                    Err(e) => return Err(format!("Database error: {}", e)),
                };

                let total_completed_today = tasks
                    .iter()
                    .filter(|t| {
                        t.completed_at
                            .map(|dt| {
                                let today = chrono::Local::now().date_naive();
                                dt.with_timezone(&chrono::Local).date_naive() == today
                            })
                            .unwrap_or(false)
                    })
                    .count();

                Ok(DashboardData {
                    total_pending: pending_tasks.len(),
                    total_in_progress: in_progress.len(),
                    total_completed_today,
                    in_progress,
                    pending_tasks,
                    recent_records,
                })
            }
            None => Err("Database not initialized".to_string()),
        }
    }

    async fn handle_start_task(&self, id: uuid::Uuid) -> Result<(), String> {
        eprintln!("[Store] handle_start_task called for id: {}", id);
        match &self.db {
            Some(db) => {
                let mut record = match db.get_record_by_id(id) {
                    Ok(Some(record)) => record,
                    Ok(None) => return Err("Task not found".to_string()),
                    Err(e) => return Err(format!("Database error: {}", e)),
                };

                record.status = Some(crate::models::TaskStatus::InProgress);
                record.updated_at = chrono::Utc::now();
                db.create_record(&record)
                    .map_err(|e| format!("Database error: {}", e))
            }
            None => Err("Database not initialized".to_string()),
        }
    }

    async fn handle_complete_task(&self, id: uuid::Uuid) -> Result<(), String> {
        eprintln!("[Store] handle_complete_task called for id: {}", id);
        match &self.db {
            Some(db) => db
                .mark_task_completed(id, chrono::Utc::now())
                .map_err(|e| format!("Database error: {}", e)),
            None => Err("Database not initialized".to_string()),
        }
    }

    async fn handle_cancel_task(
        &self,
        id: uuid::Uuid,
        reason: Option<String>,
    ) -> Result<(), String> {
        eprintln!("[Store] handle_cancel_task called for id: {}", id);
        match &self.db {
            Some(db) => {
                let mut record = match db.get_record_by_id(id) {
                    Ok(Some(record)) => record,
                    Ok(None) => return Err("Task not found".to_string()),
                    Err(e) => return Err(format!("Database error: {}", e)),
                };

                record.status = Some(crate::models::TaskStatus::Cancelled);
                record.cancelled_reason = reason;
                record.updated_at = chrono::Utc::now();
                db.create_record(&record)
                    .map_err(|e| format!("Database error: {}", e))
            }
            None => Err("Database not initialized".to_string()),
        }
    }

    async fn handle_get_all_tags(&self) -> Result<Vec<Tag>, String> {
        eprintln!("[Store] handle_get_all_tags called");
        match &self.db {
            Some(db) => match db.get_tags() {
                Ok(tags) => {
                    eprintln!("[Store] Found {} tags", tags.len());
                    Ok(tags)
                }
                Err(e) => {
                    eprintln!("[Store] Get tags query failed: {}", e);
                    Err(format!("Database error: {}", e))
                }
            },
            None => Err("Database not initialized".to_string()),
        }
    }

    async fn handle_create_tag(&self, name: String) -> Result<i64, String> {
        eprintln!("[Store] handle_create_tag called with name='{}'", name);
        match &self.db {
            Some(db) => match db.create_tag(&name, None) {
                Ok(tag) => {
                    eprintln!("[Store] Created tag with id: {}", tag.id);
                    Ok(tag.id)
                }
                Err(e) => {
                    eprintln!("[Store] Create tag failed: {}", e);
                    Err(format!("Database error: {}", e))
                }
            },
            None => Err("Database not initialized".to_string()),
        }
    }

    async fn handle_add_tag_to_record(
        &self,
        record_id: uuid::Uuid,
        tag_id: i64,
    ) -> Result<(), String> {
        eprintln!(
            "[Store] handle_add_tag_to_record called for record_id: {}, tag_id: {}",
            record_id, tag_id
        );
        match &self.db {
            Some(db) => db
                .add_tag_to_record(record_id, tag_id)
                .map_err(|e| format!("Database error: {}", e)),
            None => Err("Database not initialized".to_string()),
        }
    }

    async fn handle_get_all_persons(&self) -> Result<Vec<Person>, String> {
        eprintln!("[Store] handle_get_all_persons called");
        match &self.db {
            Some(db) => match db.get_persons() {
                Ok(persons) => {
                    eprintln!("[Store] Found {} persons", persons.len());
                    Ok(persons)
                }
                Err(e) => {
                    eprintln!("[Store] Get persons query failed: {}", e);
                    Err(format!("Database error: {}", e))
                }
            },
            None => Err("Database not initialized".to_string()),
        }
    }

    async fn handle_create_person(&self, name: String) -> Result<i64, String> {
        eprintln!("[Store] handle_create_person called with name='{}'", name);
        match &self.db {
            Some(db) => match db.create_person(&name) {
                Ok(person) => {
                    eprintln!("[Store] Created person with id: {}", person.id);
                    Ok(person.id)
                }
                Err(e) => {
                    eprintln!("[Store] Create person failed: {}", e);
                    Err(format!("Database error: {}", e))
                }
            },
            None => Err("Database not initialized".to_string()),
        }
    }

    async fn handle_add_person_to_record(
        &self,
        record_id: uuid::Uuid,
        person_id: i64,
    ) -> Result<(), String> {
        eprintln!(
            "[Store] handle_add_person_to_record called for record_id: {}, person_id: {}",
            record_id, person_id
        );
        match &self.db {
            Some(db) => db
                .add_person_to_record(record_id, person_id)
                .map_err(|e| format!("Database error: {}", e)),
            None => Err("Database not initialized".to_string()),
        }
    }
}

pub fn create_store() -> (Store, StoreRuntime) {
    let (sender, receiver) = unbounded();
    let store = Store { sender };
    let runtime = StoreRuntime::new(receiver);
    (store, runtime)
}

impl Store {
    pub async fn get_tasks(&self, completed: bool) -> Result<Vec<Record>, String> {
        eprintln!("[Store] get_tasks called with completed={}", completed);
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::GetTasks {
                completed,
                respond_to: tx,
            })
            .await;
        let result = rx.recv().await.unwrap_or_else(|_| Ok(Vec::new()));
        eprintln!(
            "[Store] get_tasks returning: {:?} records",
            result.as_ref().map(|v| v.len())
        );
        result
    }

    pub async fn create_record(&self, record: Record) -> Result<(), String> {
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::CreateRecord {
                record,
                respond_to: tx,
            })
            .await;
        rx.recv().await.unwrap_or(Ok(()))
    }

    pub async fn update_record(&self, record: Record) -> Result<(), String> {
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::UpdateRecord {
                record,
                respond_to: tx,
            })
            .await;
        rx.recv().await.unwrap_or(Ok(()))
    }

    pub async fn get_notes(&self) -> Result<Vec<Record>, String> {
        eprintln!("[Store] get_notes called");
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::GetNotes { respond_to: tx })
            .await;
        let result = rx.recv().await.unwrap_or_else(|_| Ok(Vec::new()));
        eprintln!(
            "[Store] get_notes returning: {:?} records",
            result.as_ref().map(|v| v.len())
        );
        result
    }

    pub async fn delete_record(&self, id: uuid::Uuid) -> Result<(), String> {
        eprintln!("[Store] delete_record called for id: {}", id);
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::DeleteRecord { id, respond_to: tx })
            .await;
        rx.recv().await.unwrap_or(Ok(()))
    }

    pub async fn get_timeline(&self, limit: usize, offset: usize) -> Result<Vec<Record>, String> {
        eprintln!(
            "[Store] get_timeline called with limit={}, offset={}",
            limit, offset
        );
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::GetTimeline {
                limit,
                offset,
                respond_to: tx,
            })
            .await;
        let result = rx.recv().await.unwrap_or_else(|_| Ok(Vec::new()));
        eprintln!(
            "[Store] get_timeline returning: {:?} records",
            result.as_ref().map(|v| v.len())
        );
        result
    }

    pub async fn search_records(&self, query: &str) -> Result<Vec<Record>, String> {
        eprintln!("[Store] search_records called with query='{}'", query);
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::SearchRecords {
                query: query.to_string(),
                respond_to: tx,
            })
            .await;
        let result = rx.recv().await.unwrap_or_else(|_| Ok(Vec::new()));
        eprintln!(
            "[Store] search_records returning: {:?} records",
            result.as_ref().map(|v| v.len())
        );
        result
    }

    pub async fn get_records_by_tag(&self, tag: &str) -> Result<Vec<Record>, String> {
        eprintln!("[Store] get_records_by_tag called with tag='{}'", tag);
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::GetRecordsByTag {
                tag: tag.to_string(),
                respond_to: tx,
            })
            .await;
        let result = rx.recv().await.unwrap_or_else(|_| Ok(Vec::new()));
        eprintln!(
            "[Store] get_records_by_tag returning: {:?} records",
            result.as_ref().map(|v| v.len())
        );
        result
    }

    pub async fn get_records_by_person(&self, person: &str) -> Result<Vec<Record>, String> {
        eprintln!(
            "[Store] get_records_by_person called with person='{}'",
            person
        );
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::GetRecordsByPerson {
                person: person.to_string(),
                respond_to: tx,
            })
            .await;
        let result = rx.recv().await.unwrap_or_else(|_| Ok(Vec::new()));
        eprintln!(
            "[Store] get_records_by_person returning: {:?} records",
            result.as_ref().map(|v| v.len())
        );
        result
    }

    pub async fn get_dashboard(&self) -> Result<DashboardData, String> {
        eprintln!("[Store] get_dashboard called");
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::GetDashboard { respond_to: tx })
            .await;
        rx.recv()
            .await
            .unwrap_or_else(|_| Err("Failed to get dashboard".to_string()))
    }

    pub async fn start_task(&self, id: uuid::Uuid) -> Result<(), String> {
        eprintln!("[Store] start_task called for id: {}", id);
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::StartTask { id, respond_to: tx })
            .await;
        rx.recv().await.unwrap_or(Ok(()))
    }

    pub async fn complete_task(&self, id: uuid::Uuid) -> Result<(), String> {
        eprintln!("[Store] complete_task called for id: {}", id);
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::CompleteTask { id, respond_to: tx })
            .await;
        rx.recv().await.unwrap_or(Ok(()))
    }

    pub async fn cancel_task(&self, id: uuid::Uuid, reason: Option<String>) -> Result<(), String> {
        eprintln!("[Store] cancel_task called for id: {}", id);
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::CancelTask {
                id,
                reason,
                respond_to: tx,
            })
            .await;
        rx.recv().await.unwrap_or(Ok(()))
    }

    pub async fn get_all_tags(&self) -> Result<Vec<Tag>, String> {
        eprintln!("[Store] get_all_tags called");
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::GetAllTags { respond_to: tx })
            .await;
        let result = rx.recv().await.unwrap_or_else(|_| Ok(Vec::new()));
        eprintln!(
            "[Store] get_all_tags returning: {:?} tags",
            result.as_ref().map(|v| v.len())
        );
        result
    }

    pub async fn create_tag(&self, name: &str) -> Result<i64, String> {
        eprintln!("[Store] create_tag called with name='{}'", name);
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::CreateTag {
                name: name.to_string(),
                respond_to: tx,
            })
            .await;
        rx.recv()
            .await
            .unwrap_or_else(|_| Err("Failed to create tag".to_string()))
    }

    pub async fn add_tag_to_record(
        &self,
        record_id: uuid::Uuid,
        tag_id: i64,
    ) -> Result<(), String> {
        eprintln!(
            "[Store] add_tag_to_record called for record_id: {}, tag_id: {}",
            record_id, tag_id
        );
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::AddTagToRecord {
                record_id,
                tag_id,
                respond_to: tx,
            })
            .await;
        rx.recv().await.unwrap_or(Ok(()))
    }

    pub async fn get_all_persons(&self) -> Result<Vec<Person>, String> {
        eprintln!("[Store] get_all_persons called");
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::GetAllPersons { respond_to: tx })
            .await;
        let result = rx.recv().await.unwrap_or_else(|_| Ok(Vec::new()));
        eprintln!(
            "[Store] get_all_persons returning: {:?} persons",
            result.as_ref().map(|v| v.len())
        );
        result
    }

    pub async fn create_person(&self, name: &str) -> Result<i64, String> {
        eprintln!("[Store] create_person called with name='{}'", name);
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::CreatePerson {
                name: name.to_string(),
                respond_to: tx,
            })
            .await;
        rx.recv()
            .await
            .unwrap_or_else(|_| Err("Failed to create person".to_string()))
    }

    pub async fn add_person_to_record(
        &self,
        record_id: uuid::Uuid,
        person_id: i64,
    ) -> Result<(), String> {
        eprintln!(
            "[Store] add_person_to_record called for record_id: {}, person_id: {}",
            record_id, person_id
        );
        let (tx, rx) = async_channel::unbounded();
        let _ = self
            .sender
            .send(StoreCommand::AddPersonToRecord {
                record_id,
                person_id,
                respond_to: tx,
            })
            .await;
        rx.recv().await.unwrap_or(Ok(()))
    }
}
