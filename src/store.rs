use crate::models::Record;
use async_channel::{Sender, Receiver, unbounded};
use std::sync::Arc;

#[derive(Clone)]
pub struct Store {
    sender: Sender<StoreCommand>,
}

pub enum StoreCommand {
    GetTasks { completed: bool, respond_to: Sender<Result<Vec<Record>, String>> },
    CreateRecord { record: Record, respond_to: Sender<Result<(), String>> },
    UpdateRecord { record: Record, respond_to: Sender<Result<(), String>> },
}

pub struct StoreRuntime {
    receiver: Receiver<StoreCommand>,
}

impl StoreRuntime {
    pub fn new(receiver: Receiver<StoreCommand>) -> Self {
        Self { receiver }
    }

    pub async fn run(&self, _db_path: std::path::PathBuf) {
        while let Ok(cmd) = self.receiver.recv().await {
            match cmd {
                StoreCommand::GetTasks { completed: _, respond_to } => {
                    let _ = respond_to.send(Ok(Vec::new())).await;
                }
                StoreCommand::CreateRecord { record: _, respond_to } => {
                    let _ = respond_to.send(Ok(())).await;
                }
                StoreCommand::UpdateRecord { record: _, respond_to } => {
                    let _ = respond_to.send(Ok(())).await;
                }
            }
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
        let (tx, rx) = async_channel::unbounded();
        let _ = self.sender.send(StoreCommand::GetTasks { completed, respond_to: tx }).await;
        rx.recv().await.unwrap_or_else(|_| Ok(Vec::new()))
    }

    pub async fn create_record(&self, record: Record) -> Result<(), String> {
        let (tx, rx) = async_channel::unbounded();
        let _ = self.sender.send(StoreCommand::CreateRecord { record, respond_to: tx }).await;
        rx.recv().await.unwrap_or_else(|_| Ok(()))
    }

    pub async fn update_record(&self, record: Record) -> Result<(), String> {
        let (tx, rx) = async_channel::unbounded();
        let _ = self.sender.send(StoreCommand::UpdateRecord { record, respond_to: tx }).await;
        rx.recv().await.unwrap_or_else(|_| Ok(()))
    }
}
