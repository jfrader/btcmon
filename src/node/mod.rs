pub mod providers;

use crate::{app::AppThread, config::AppConfig};
use anyhow::Result;
use async_trait::async_trait;
use std::{
    collections::HashMap,
    fmt,
    marker::Sized,
    sync::{Arc, Mutex},
};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    time::Instant,
};

pub struct NodeChannel<E> {
    pub sender: UnboundedSender<E>,
    pub receiver: Arc<Mutex<UnboundedReceiver<E>>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NodeStatus {
    Online,
    Offline,
    Connecting,
    Synchronizing,
}

impl fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub enum NodeEvent {
    NewBlock(String),
    Status(NodeStatus),
    State(NodeState),
}

#[derive(Clone, Debug)]
pub struct NodeState {
    pub title: String,
    pub alias: String,
    pub host: String,
    pub message: String,
    pub status: NodeStatus,
    pub height: u64,
    pub headers: u64,
    pub last_hash: String,
    pub last_hash_instant: Option<Instant>,
    pub services: HashMap<String, NodeStatus>,
    pub service_display_index: usize,
    pub last_service_switch: Option<Instant>,
}

impl Default for NodeState {
    fn default() -> Self {
        Self {
            title: "".to_string(),
            alias: "".to_string(),
            host: "".to_string(),
            message: "".to_string(),
            status: NodeStatus::Offline,
            height: 0,
            headers: 0,
            last_hash: "".to_string(),
            last_hash_instant: None,
            services: HashMap::new(),
            last_service_switch: None,
            service_display_index: 0,
        }
    }
}

impl NodeState {
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::default()))
    }

    pub fn set_last_service_switch(&mut self, instant: Option<Instant>, service_display_index: usize) {
        self.last_service_switch = instant;
        self.service_display_index = service_display_index;
    }
}

#[async_trait]
pub trait NodeProvider {
    fn new(config: &AppConfig) -> Self
    where
        Self: Sized;
    async fn init(&mut self, thread: AppThread) -> Result<()>;
    fn get_state(&self) -> Arc<Mutex<NodeState>>;
}

pub struct Node {
    pub thread: AppThread,
    handler: Option<tokio::task::JoinHandle<()>>,
}

impl Node {
    pub fn new(thread: AppThread) -> Self {
        Self {
            thread: thread.clone(),
            handler: None,
        }
    }

    pub fn init(&mut self, mut provider: Box<dyn NodeProvider + Send + 'static>) {
        if let Some(handler) = &self.handler {
            handler.abort();
        }

        let token = self.thread.token.clone();
        let thread = self.thread.clone();
        self.handler = Some(self.thread.tracker.spawn(async move {
            tokio::select! {
                _ = provider.init(thread) => {},
                () = token.cancelled() => {},
            }
        }));
    }
}
