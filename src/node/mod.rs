// node/mod.rs

pub mod providers;
pub mod widgets;

use crate::{
    app::AppThread,
    config::AppConfig,
    widget::{DefaultWidgetState, DynamicState},
};
use anyhow::Result;
use async_trait::async_trait;
use ratatui::{
    layout::Alignment,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
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
use tui_widgets::popup::{Popup, SizedWrapper};

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

impl Default for NodeStatus {
    fn default() -> Self {
        NodeStatus::Offline
    }
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

#[derive(Debug)]
pub struct NodeState {
    pub host: String,
    pub message: String,
    pub status: NodeStatus,
    pub height: u64,
    pub last_hash_instant: Option<Instant>,
    pub services: HashMap<String, NodeStatus>,
    pub service_display_index: usize,
    pub last_service_switch: Option<Instant>,
    pub widget_state: Box<dyn DynamicState>,
}

impl Clone for NodeState {
    fn clone(&self) -> Self {
        Self {
            host: self.host.clone(),
            message: self.message.clone(),
            status: self.status,
            height: self.height,
            last_hash_instant: self.last_hash_instant,
            services: self.services.clone(),
            last_service_switch: self.last_service_switch,
            service_display_index: self.service_display_index,
            widget_state: self.widget_state.clone_box(),
        }
    }
}

impl Default for NodeState {
    fn default() -> Self {
        Self {
            host: "".to_string(),
            message: "".to_string(),
            status: NodeStatus::Offline,
            height: 0,
            last_hash_instant: None,
            services: HashMap::new(),
            last_service_switch: None,
            service_display_index: 0,
            widget_state: Box::new(DefaultWidgetState),
        }
    }
}

impl NodeState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_last_service_switch(
        &mut self,
        instant: Option<Instant>,
        service_display_index: usize,
    ) {
        self.last_service_switch = instant;
        self.service_display_index = service_display_index;
    }

    pub fn draw_new_block_popup(&self, frame: &mut Frame, block_height: u64) {
        let sized_paragraph = SizedWrapper {
            inner: Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![Span::raw("Height")]),
                Line::from(vec![Span::raw(block_height.to_string())]),
                Line::from(""),
            ])
            .alignment(Alignment::Center),
            width: 21,
            height: 4,
        };

        let popup = Popup::new(sized_paragraph)
            .title(" New block! ")
            .style(Style::new().fg(Color::White));
        frame.render_widget(&popup, frame.area());
    }
}

#[async_trait]
pub trait NodeProvider {
    fn new(config: &AppConfig) -> Self
    where
        Self: Sized;
    async fn init(&mut self, thread: AppThread) -> Result<()>;
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
