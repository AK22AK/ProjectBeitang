use crate::db::Database;
use crate::models::Record;
use anyhow::Result;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum StoreCommand {
    CreateRecord(Record),
    GetRecord(Uuid),
    GetAllRecords,
    GetTasks { include_completed: bool },
    UpdateRecord(Record),
    DeleteRecord(Uuid),
}

#[derive(Debug, Clone)]
pub enum StoreResponse {
    RecordCreated(Uuid),
    Record(Option<Record>),
    Records(Vec<Record>),
    RecordUpdated,
    RecordDeleted,
    Error(String),
}

pub type StoreSender = async_channel::Sender<(StoreCommand, async_channel::Sender<StoreResponse>)>;
pub type StoreReceiver = async_channel::Receiver<(StoreCommand, async_channel::Sender<StoreResponse>)>;

pub struct StoreRuntime {
    receiver: StoreReceiver,
}

impl StoreRuntime {
    pub fn new(receiver: StoreReceiver) -> Self {
        Self { receiver }
    }

    pub async fn run<P: AsRef<Path>>(self, db_path: P) -> Result<()> {
        let db = Database::open(db_path)?;

        while let Ok((command, responder)) = self.receiver.recv().await {
            let response = Self::handle_command(&db, command);
            let _ = responder.send(response).await;
        }

        Ok(())
    }

    fn handle_command(db: &Database, command: StoreCommand) -> StoreResponse {
        match command {
            StoreCommand::CreateRecord(record) => {
                let id = record.id;
                match db.insert_record(&record) {
                    Ok(_) => StoreResponse::RecordCreated(id),
                    Err(e) => StoreResponse::Error(e.to_string()),
                }
            }
            StoreCommand::GetRecord(id) => {
                match db.get_record(id) {
                    Ok(record) => StoreResponse::Record(record),
                    Err(e) => StoreResponse::Error(e.to_string()),
                }
            }
            StoreCommand::GetAllRecords => {
                match db.get_all_records() {
                    Ok(records) => StoreResponse::Records(records),
                    Err(e) => StoreResponse::Error(e.to_string()),
                }
            }
            StoreCommand::GetTasks { include_completed } => {
                match db.get_tasks(include_completed) {
                    Ok(records) => StoreResponse::Records(records),
                    Err(e) => StoreResponse::Error(e.to_string()),
                }
            }
            StoreCommand::UpdateRecord(record) => {
                match db.update_record(&record) {
                    Ok(_) => StoreResponse::RecordUpdated,
                    Err(e) => StoreResponse::Error(e.to_string()),
                }
            }
            StoreCommand::DeleteRecord(id) => {
                match db.delete_record(id) {
                    Ok(_) => StoreResponse::RecordDeleted,
                    Err(e) => StoreResponse::Error(e.to_string()),
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct Store {
    sender: StoreSender,
}

impl Store {
    pub fn new(sender: StoreSender) -> Self {
        Self { sender }
    }

    pub async fn create_record(&self, record: Record) -> Result<Uuid> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender.send((StoreCommand::CreateRecord(record), tx)).await?;

        match rx.recv().await? {
            StoreResponse::RecordCreated(id) => Ok(id),
            StoreResponse::Error(e) => Err(anyhow::anyhow!(e)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn get_record(&self, id: Uuid) -> Result<Option<Record>> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender.send((StoreCommand::GetRecord(id), tx)).await?;

        match rx.recv().await? {
            StoreResponse::Record(record) => Ok(record),
            StoreResponse::Error(e) => Err(anyhow::anyhow!(e)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn get_all_records(&self) -> Result<Vec<Record>> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender.send((StoreCommand::GetAllRecords, tx)).await?;

        match rx.recv().await? {
            StoreResponse::Records(records) => Ok(records),
            StoreResponse::Error(e) => Err(anyhow::anyhow!(e)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn get_tasks(&self, include_completed: bool) -> Result<Vec<Record>> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender.send((StoreCommand::GetTasks { include_completed }, tx)).await?;

        match rx.recv().await? {
            StoreResponse::Records(records) => Ok(records),
            StoreResponse::Error(e) => Err(anyhow::anyhow!(e)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn update_record(&self, record: Record) -> Result<()> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender.send((StoreCommand::UpdateRecord(record), tx)).await?;

        match rx.recv().await? {
            StoreResponse::RecordUpdated => Ok(()),
            StoreResponse::Error(e) => Err(anyhow::anyhow!(e)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn delete_record(&self, id: Uuid) -> Result<()> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender.send((StoreCommand::DeleteRecord(id), tx)).await?;

        match rx.recv().await? {
            StoreResponse::RecordDeleted => Ok(()),
            StoreResponse::Error(e) => Err(anyhow::anyhow!(e)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }
}

pub fn create_store() -> (Store, StoreRuntime) {
    let (sender, receiver) = async_channel::unbounded();
    let store = Store::new(sender);
    let runtime = StoreRuntime::new(receiver);
    (store, runtime)
}
