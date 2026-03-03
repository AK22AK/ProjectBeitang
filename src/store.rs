use crate::db::Database;
use crate::models::Record;
use async_channel::{Receiver, Sender, unbounded};
use std::path::PathBuf;

#[derive(Clone)]
pub struct Store {
    sender: Sender<StoreCommand>,
}

pub enum StoreCommand {
    GetTasks {
        completed: bool,
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
}

pub struct StoreRuntime {
    receiver: Receiver<StoreCommand>,
    db: Option<Database>,
}

impl StoreRuntime {
    pub fn new(receiver: Receiver<StoreCommand>) -> Self {
        Self {
            receiver,
            db: None,
        }
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
                StoreCommand::CreateRecord { record, respond_to } => {
                    let result = self.handle_create_record(record).await;
                    let _ = respond_to.send(result).await;
                }
                StoreCommand::UpdateRecord { record, respond_to } => {
                    let result = self.handle_update_record(record).await;
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
        eprintln!("[Store] get_tasks returning: {:?} records", result.as_ref().map(|v| v.len()));
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
        rx.recv().await.unwrap_or_else(|_| Ok(()))
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
        rx.recv().await.unwrap_or_else(|_| Ok(()))
    }
}
