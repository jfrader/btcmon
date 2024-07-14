pub mod providers;

use crate::{app::AppThread, config::AppConfig};
use anyhow::Result;
use async_trait::async_trait;
use std::{
    fmt,
    marker::Sized,
    sync::{Arc, Mutex},
};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    time::Instant,
};

pub enum NodeKind {
    BitcoinCore,
    CLightning,
    LND,
}

pub struct NodeChannel<E> {
    pub sender: UnboundedSender<E>,
    pub receiver: Arc<Mutex<UnboundedReceiver<E>>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NodeStatus {
    Online,
    Offline,
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
    pub status: NodeStatus,
    pub height: u64,
    pub headers: u64,
    pub last_hash: String,
    pub last_hash_instant: Option<Instant>,
}

impl Default for NodeState {
    fn default() -> Self {
        Self {
            status: NodeStatus::Offline,
            height: 0,
            headers: 0,
            last_hash: "".to_string(),
            last_hash_instant: None,
        }
    }
}

impl NodeState {
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::default()))
    }
}

#[async_trait]
pub trait NodeProvider {
    fn new(config: AppConfig) -> Self
    where
        Self: Sized;
    async fn init(&mut self, thread: AppThread) -> Result<()>;
    fn get_state(&self) -> Arc<Mutex<NodeState>>;
}

pub struct Node {
    pub thread: AppThread,
}

impl Node {
    pub fn new(thread: AppThread) -> Self {
        Self {
            thread: thread.clone(),
        }
    }
}

impl Node {
    pub fn init(
        &mut self,
        mut provider: Box<dyn NodeProvider + Send + 'static>,
    ) -> tokio::task::JoinHandle<()> {
        let token = self.thread.token.clone();
        let thread = self.thread.clone();
        self.thread.tracker.spawn(async move {
            tokio::select! {
                _ = provider.init(thread) => {},
                () = token.cancelled() => {},
            };
        })
    }
}
