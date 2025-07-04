// node/bitcoin_core.rs

use anyhow::Result;
use async_trait::async_trait;
use bitcoin::consensus::deserialize;
use bitcoin::BlockHash;
use bitcoincore_rpc::{json::GetBlockchainInfoResult, RpcApi};
use bitcoincore_zmq::subscribe_async_monitor_stream::MessageStream;
use bitcoincore_zmq::{subscribe_async_wait_handshake, SocketEvent, SocketMessage};
use futures::StreamExt;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding, Paragraph, Widget};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time;
use tokio::time::Instant;

use crate::event::Event;
use crate::node::NodeState;
use crate::ui::get_status_style;
use crate::widget::{DynamicNodeStatefulWidget, DynamicState};
use crate::{
    app::AppThread,
    config::AppConfig,
    node::{NodeProvider, NodeStatus},
};

#[derive(Clone)]
pub struct BitcoinCore {
    rpc_client: Arc<bitcoincore_rpc::Client>,
    zmq_url: Option<String>,
    host: String,
}

#[derive(Clone, Debug, Default)]
pub struct BitcoinCoreWidgetState {
    pub title: String,
    pub headers: u64,
    pub last_hash: String,
}

impl DynamicState for BitcoinCoreWidgetState {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn clone_box(&self) -> Box<dyn DynamicState> {
        Box::new(self.clone())
    }
}

pub struct BitcoinCoreWidget;

impl DynamicNodeStatefulWidget for BitcoinCoreWidget {
    fn render_dynamic(
        &self,
        area: Rect,
        buf: &mut Buffer,
        node_state: &NodeState,
        state: &mut dyn DynamicState,
    ) {
        let mut default = BitcoinCoreWidgetState::default();
        let state = state
            .as_any_mut()
            .downcast_mut::<BitcoinCoreWidgetState>()
            .unwrap_or(&mut default);

        let style = get_status_style(&node_state.status);
        let block_height = match node_state.status {
            NodeStatus::Synchronizing => Line::from(vec![
                Span::raw("Block Height: "),
                Span::styled(node_state.height.to_string(), Style::new().fg(Color::White)),
                Span::raw("/"),
                Span::styled(state.headers.to_string(), Style::new().fg(Color::Blue)),
            ]),
            _ => Line::from(vec![
                Span::raw("Block Height: "),
                Span::styled(node_state.height.to_string(), Style::new().fg(Color::White)),
            ]),
        };

        let text = vec![
            block_height,
            Line::from(vec![
                Span::raw("Last Block: "),
                Span::styled(state.last_hash.clone(), Style::new().fg(Color::White)),
            ]),
            "------".into(),
        ];

        Paragraph::new(text)
            .block(
                Block::bordered()
                    .padding(Padding::left(1))
                    .title(state.title.clone())
                    .title_alignment(Alignment::Center)
                    .border_type(BorderType::Plain)
                    .style(style),
            )
            .render(area, buf);
    }
}

impl BitcoinCore {
    fn get_op_return_data(&self, block_hash: &str) -> Result<String> {
        let block_hex = self
            .rpc_client
            .get_block_hex(&BlockHash::from_str(block_hash)?)?;
        let block_bytes = hex::decode(&block_hex)?;
        let block: bitcoin::Block = deserialize(&block_bytes)?;

        let mut op_returns = Vec::new();

        for tx in block.txdata {
            for (_index, output) in tx.output.iter().enumerate() {
                if output.script_pubkey.is_op_return() {
                    if let Some(bytes) = output.script_pubkey.as_bytes().get(1..) {
                        if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                            if !text.is_empty() {
                                op_returns.push(text);
                            }
                        }
                    }
                }
            }
        }

        Ok(if op_returns.is_empty() {
            "".to_string()
        } else {
            op_returns.join(" | ")
        })
    }

    async fn get_blockchain_info(
        &mut self,
        sender: UnboundedSender<Event>,
    ) -> Result<GetBlockchainInfoResult> {
        let _ = sender.send(Event::NodeUpdate(Arc::new(|mut state| {
            if state.status == NodeStatus::Offline {
                state.status = NodeStatus::Connecting;
                *state
                    .services
                    .entry("RPC".to_string())
                    .or_insert(NodeStatus::Connecting) = NodeStatus::Connecting;
            }
            state
        })));

        match self.rpc_client.get_blockchain_info() {
            Ok(blockchain_info) => {
                let _ = sender.send(Event::NodeUpdate(Arc::new(move |mut state| {
                    if state.services.get("ZMQ") != Some(&NodeStatus::Online)
                        && state.height > 0
                        && state.height < blockchain_info.blocks
                    {
                        state.last_hash_instant = Some(Instant::now());
                    }

                    let new_status = if blockchain_info.blocks < blockchain_info.headers {
                        NodeStatus::Synchronizing
                    } else {
                        NodeStatus::Online
                    };

                    state.status = new_status;
                    state.message = "".to_string();
                    state.height = blockchain_info.blocks;

                    *state
                        .services
                        .entry("RPC".to_string())
                        .or_insert(new_status) = new_status;

                    state.widget_state = Box::new(BitcoinCoreWidgetState {
                        title: "Bitcoin Core".to_string(),
                        headers: blockchain_info.headers,
                        last_hash: blockchain_info.best_block_hash.to_string(),
                    });

                    state
                })));

                Ok(blockchain_info)
            }
            Err(e) => Err(e.into()),
        }
    }

    fn spawn_zmq_listener(
        &self,
        thread: &AppThread,
        mut stream: MessageStream,
    ) -> tokio::task::JoinHandle<()> {
        let token = thread.token.clone();
        let sender = thread.sender.clone();

        let _ = sender.send(Event::NodeUpdate(Arc::new(|mut state| {
            *state
                .services
                .entry("ZMQ".to_string())
                .or_insert(NodeStatus::Online) = NodeStatus::Online;

            state
        })));

        thread.tracker.spawn(async move {
            loop {
                let recv = tokio::select! {
                    r = stream.next() => r,
                    () = token.cancelled() => None
                };

                if let Some(ref msg) = recv {
                    match msg {
                        Ok(SocketMessage::Message(msg)) => match msg {
                            bitcoincore_zmq::Message::HashBlock(hash, _) => {
                                let hash = hash.to_string();

                                let _ =
                                    sender.send(Event::NodeUpdate(Arc::new(move |mut state| {
                                        let widget_state = state
                                            .widget_state
                                            .as_any()
                                            .downcast_ref::<BitcoinCoreWidgetState>()
                                            .unwrap();
                                        if widget_state.last_hash != hash {
                                            state.height += 1;
                                            state.widget_state = Box::new(BitcoinCoreWidgetState {
                                                title: widget_state.title.clone(),
                                                headers: widget_state.headers,
                                                last_hash: hash.clone(),
                                            });
                                        }

                                        state.last_hash_instant = Some(Instant::now());
                                        state
                                    })));
                            }
                            _ => {}
                        },
                        Ok(SocketMessage::Event(event)) => match event.event {
                            SocketEvent::Disconnected { .. } => {
                                let _ = sender.send(Event::NodeUpdate(Arc::new(|current| {
                                    let mut state = current.clone();
                                    *state
                                        .services
                                        .entry("ZMQ".to_string())
                                        .or_insert(NodeStatus::Offline) = NodeStatus::Offline;

                                    state
                                })));
                            }
                            SocketEvent::HandshakeSucceeded => {
                                let _ = sender.send(Event::NodeUpdate(Arc::new(|current| {
                                    let mut state = current.clone();
                                    *state
                                        .services
                                        .entry("ZMQ".to_string())
                                        .or_insert(NodeStatus::Online) = NodeStatus::Online;

                                    state
                                })));
                            }
                            _ => {}
                        },
                        Err(_) => {
                            break;
                        }
                    }
                } else {
                    break;
                }
            }

            let _ = sender.send(Event::NodeUpdate(Arc::new(|current| {
                let mut state = current.clone();
                *state
                    .services
                    .entry("ZMQ".to_string())
                    .or_insert(NodeStatus::Offline) = NodeStatus::Offline;

                state
            })));
        })
    }

    async fn subscribe(
        &mut self,
        thread: &AppThread,
        zmq_url: &str,
    ) -> Result<tokio::task::JoinHandle<()>> {
        let urls = [zmq_url];

        let sender = thread.sender.clone();

        let _ = sender.send(Event::NodeUpdate(Arc::new(|mut state| {
            *state
                .services
                .entry("ZMQ".to_string())
                .or_insert(NodeStatus::Connecting) = NodeStatus::Connecting;

            state
        })));

        let select = tokio::select! {
            r = tokio::time::timeout(
                tokio::time::Duration::from_millis(5000),
                subscribe_async_wait_handshake(&urls),
            ) => r.ok(),
            () = thread.token.cancelled() => None
        };

        let stream = match select {
            Some(Ok(stream)) => stream,
            _ => {
                return Err(anyhow::Error::msg("Failed to subscribe to ZMQ"));
            }
        };

        Ok(self.spawn_zmq_listener(thread, stream))
    }

    async fn try_subscribe(
        &mut self,
        thread: &AppThread,
    ) -> Option<Result<tokio::task::JoinHandle<()>>> {
        if let Some(url) = self.zmq_url.clone() {
            return Some(self.subscribe(thread, &url).await);
        };

        None
    }
}

#[async_trait]
impl NodeProvider for BitcoinCore {
    fn new(config: &AppConfig) -> Self {
        let rpc = bitcoincore_rpc::Client::new(
            vec![
                config.bitcoin_core.host.as_str(),
                config.bitcoin_core.rpc_port.as_str(),
            ]
            .join(":")
            .as_str(),
            bitcoincore_rpc::Auth::UserPass(
                config.bitcoin_core.rpc_user.to_string(),
                config.bitcoin_core.rpc_password.to_string(),
            ),
        )
        .unwrap();

        let zmq_url: Option<String> = match config.bitcoin_core.host.as_str() {
            "" => None,
            _ => Some(
                vec![
                    "tcp://",
                    &config.bitcoin_core.host,
                    ":",
                    &config.bitcoin_core.zmq_port,
                ]
                .join(""),
            ),
        };

        Self {
            rpc_client: Arc::new(rpc),
            zmq_url,
            host: config.bitcoin_core.host.clone(),
        }
    }

    async fn init(&mut self, thread: AppThread) -> Result<()> {
        let check_interval = time::Duration::from_millis(15 * 1000);

        let host = self.host.clone();

        let _ = thread
            .sender
            .send(Event::NodeUpdate(Arc::new(move |mut state| {
                state.host = host.clone();
                state.message = "Initializing Bitcoin Core...".to_string();
                state.widget_state = Box::new(BitcoinCoreWidgetState {
                    title: "Bitcoin Core".to_string(),
                    headers: 0,
                    last_hash: "".to_string(),
                });
                state
                    .services
                    .insert("RPC".to_string(), NodeStatus::Offline);
                state
                    .services
                    .insert("ZMQ".to_string(), NodeStatus::Offline);
                state
            })));

        let _ = self.get_blockchain_info(thread.sender.clone()).await;

        let mut sub_handlers = Box::new(self.try_subscribe(&thread).await);

        loop {
            if thread.token.is_cancelled() {
                break;
            }

            match *sub_handlers {
                Some(Ok(ref handler)) => {
                    if handler.is_finished() {
                        sub_handlers = Box::new(self.try_subscribe(&thread).await);
                    }
                }
                Some(Err(_)) => {
                    sub_handlers = Box::new(self.try_subscribe(&thread).await);
                }
                _ => {}
            }

            let _ = self.get_blockchain_info(thread.sender.clone()).await;

            tokio::time::sleep(check_interval).await;
        }

        Ok(())
    }
}
