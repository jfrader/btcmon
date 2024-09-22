use anyhow::Result;
use async_trait::async_trait;
use bitcoincore_rpc::{json::GetBlockchainInfoResult, RpcApi};
use bitcoincore_zmq::subscribe_async_monitor_stream::MessageStream;
use bitcoincore_zmq::{subscribe_async_wait_handshake, SocketEvent, SocketMessage};
use futures::StreamExt;
use std::sync::{Arc, Mutex};
use tokio::time;
use tokio::time::Instant;

use crate::{
    app::AppThread,
    config::AppConfig,
    node::{NodeProvider, NodeState, NodeStatus},
};

#[derive(Clone)]
pub struct BitcoinCore {
    rpc_client: Arc<bitcoincore_rpc::Client>,
    zmq_url: Option<String>,
    state: Arc<Mutex<NodeState>>,
}

impl BitcoinCore {
    async fn get_blockchain_info(&mut self) -> Result<GetBlockchainInfoResult> {
        match self.rpc_client.get_blockchain_info() {
            Ok(blockchain_info) => {
                let mut state = self.state.lock().unwrap();
                let new_status = if blockchain_info.blocks < blockchain_info.headers {
                    NodeStatus::Synchronizing
                } else {
                    NodeStatus::Online
                };

                state.status = new_status;
                state.last_hash = blockchain_info.best_block_hash.to_string();
                state.headers = blockchain_info.headers;
                state.height = blockchain_info.blocks;

                *state
                    .services
                    .entry("RPC".to_string())
                    .or_insert(new_status) = new_status;

                Ok(blockchain_info)
            }
            Err(e) => {
                let mut state = self.state.lock().unwrap();
                *state
                    .services
                    .entry("RPC".to_string())
                    .or_insert(NodeStatus::Offline) = NodeStatus::Offline;
                state.status = NodeStatus::Offline;
                Err(e.into())
            }
        }
    }

    fn spawn_zmq_listener(
        &self,
        thread: &AppThread,
        mut stream: MessageStream,
    ) -> tokio::task::JoinHandle<()> {
        let token = thread.token.clone();
        let state = self.state.clone();
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
                                let mut locked_state = state.lock().unwrap();

                                if locked_state.last_hash != hash {
                                    locked_state.height += 1;
                                    locked_state.last_hash = hash;
                                }

                                locked_state.last_hash_instant = Some(Instant::now());
                            }
                            _ => {}
                        },
                        Ok(SocketMessage::Event(event)) => match event.event {
                            SocketEvent::Disconnected { .. } => {
                                BitcoinCore::set_service_status(&state, "ZMQ", NodeStatus::Offline);
                            }
                            SocketEvent::HandshakeSucceeded => {
                                BitcoinCore::set_service_status(&state, "ZMQ", NodeStatus::Online);
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

            BitcoinCore::set_service_status(&state, "ZMQ", NodeStatus::Offline);
        })
    }

    async fn subscribe(
        &mut self,
        thread: &AppThread,
        zmq_url: &str,
    ) -> Result<tokio::task::JoinHandle<()>> {
        let urls = [zmq_url];

        let select = tokio::select! {
            r = tokio::time::timeout(
                tokio::time::Duration::from_millis(5000),
                subscribe_async_wait_handshake(&urls),
            ) => r.ok(),
            () = thread.token.cancelled() => None
        };

        let stream = match select {
            Some(Ok(stream)) => {
                BitcoinCore::set_service_status(&self.state, "ZMQ", NodeStatus::Online);
                stream
            }
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
            return Some(self.subscribe(&thread, &url).await);
        };

        None
    }

    fn set_service_status(state: &Arc<Mutex<NodeState>>, service: &str, status: NodeStatus) {
        *state
            .lock()
            .unwrap()
            .services
            .entry(service.to_string())
            .or_insert(status) = status;
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

        let state = NodeState::new();

        {
            let mut locked_state = state.lock().unwrap();

            locked_state
                .services
                .insert("RPC".to_string(), NodeStatus::Offline);

            locked_state
                .services
                .insert("ZMQ".to_string(), NodeStatus::Offline);
        }

        Self {
            rpc_client: Arc::new(rpc),
            zmq_url,
            state,
        }
    }

    async fn init(&mut self, thread: AppThread) -> Result<()> {
        let check_interval = time::Duration::from_millis(15 * 1000);

        let _ = self.get_blockchain_info().await;

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

            let _ = self.get_blockchain_info().await;

            tokio::time::sleep(check_interval).await;
        }

        Ok(())
    }

    fn get_state(&self) -> Arc<Mutex<NodeState>> {
        return self.state.clone();
    }
}
