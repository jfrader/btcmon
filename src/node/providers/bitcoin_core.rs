use anyhow::Result;
use async_trait::async_trait;
use bitcoincore_rpc::{json::GetBlockchainInfoResult, RpcApi};
use std::sync::{Arc, Mutex};
use tokio::time::Instant;
use tokio::{sync::mpsc, time};
use tokio_util::sync::CancellationToken;
use zmq::PollEvents;

use crate::{
    app::AppThread,
    config::AppConfig,
    node::{NodeProvider, NodeState, NodeStatus},
};

enum BitcoinCoreEvent {
    NewBlock(String),
}

#[derive(Clone)]
pub struct BitcoinCore {
    rpc_client: Arc<bitcoincore_rpc::Client>,
    zmq_hashblock_url: String,
    state: Arc<Mutex<NodeState>>,
}

impl BitcoinCore {
    async fn get_blockchain_info(&mut self) -> Result<GetBlockchainInfoResult> {
        match self.rpc_client.get_blockchain_info() {
            Ok(blockchain_info) => {
                let mut state = self.state.lock().unwrap();
                state.status = if blockchain_info.blocks < blockchain_info.headers {
                    NodeStatus::Synchronizing
                } else {
                    NodeStatus::Online
                };

                state.last_hash = blockchain_info.best_block_hash.to_string();
                state.headers = blockchain_info.headers;
                state.height = blockchain_info.blocks;

                Ok(blockchain_info)
            }
            Err(e) => {
                let mut state = self.state.lock().unwrap();
                state.status = NodeStatus::Offline;
                Err(e.into())
            }
        }
    }

    async fn event_handler(
        thread: AppThread,
        state: Arc<Mutex<NodeState>>,
        mut rx: mpsc::UnboundedReceiver<BitcoinCoreEvent>,
    ) -> Result<()> {
        let token = thread.token.clone();
        loop {
            let ev = tokio::select! {
                r = rx.recv() => r,
                () = token.cancelled() => {
                    None
                },
            };
            match ev {
                Some(BitcoinCoreEvent::NewBlock(hash)) => {
                    let mut locked_state = state.lock().unwrap();

                    if locked_state.last_hash != hash {
                        locked_state.height += 1;
                        locked_state.last_hash = hash;
                    }

                    locked_state.last_hash_instant = Some(Instant::now());
                }
                None => {
                    break;
                }
            }
        }

        Err(anyhow::Error::msg("Disconnected"))
    }

    fn spawn_subscription_listener(
        &mut self,
        socket: zmq::Socket,
        thread: AppThread,
        state: Arc<Mutex<NodeState>>,
    ) -> Result<(
        tokio::task::JoinHandle<Result<(), anyhow::Error>>,
        tokio::task::JoinHandle<Result<(), anyhow::Error>>,
    )> {
        let (tx, rx) = mpsc::unbounded_channel::<BitcoinCoreEvent>();
        let thread_clone = thread.clone();

        let event_handler =
            thread
                .tracker
                .spawn(BitcoinCore::event_handler(thread_clone, state, rx));

        let listener_handler = thread.tracker.spawn(BitcoinCore::hashblock_listener(
            socket,
            tx,
            thread.token.clone(),
        ));

        return Ok((listener_handler, event_handler));
    }

    fn recv_hashblock(
        socket: zmq::Socket,
    ) -> std::result::Result<(zmq::Socket, Vec<Vec<u8>>), Option<zmq::Socket>> {
        let poll = socket.poll(PollEvents::POLLIN, 2500).unwrap();
        if poll > 0 {
            if let Some(e) = socket.recv_multipart(0).ok() {
                return Ok((socket, e));
            } else {
                return Err(None);
            }
        } else {
            return Err(Some(socket));
        }
    }

    async fn hashblock_listener(
        socket: zmq::Socket,
        tx: mpsc::UnboundedSender<BitcoinCoreEvent>,
        token: CancellationToken,
    ) -> Result<()> {
        let mut sub = Some(socket);
        loop {
            if token.is_cancelled() {
                break;
            }

            if sub.is_none() {
                break;
            }

            let result = tokio::select! {
                r = tokio::task::spawn_blocking(move || {
                    Self::recv_hashblock(sub.unwrap())
                }) => r.unwrap(),
                () = token.cancelled() => Err(None),
            };

            match result {
                Ok((ret_socket, repl)) => {
                    let hash = hex::encode(repl.get(1).unwrap());
                    tx.send(BitcoinCoreEvent::NewBlock(hash.clone()))?;
                    sub = Some(ret_socket);
                }
                Err(e) => match e {
                    Some(ret_socket) => {
                        sub = Some(ret_socket);
                    }
                    _ => {
                        sub = None;
                    }
                },
            };
        }

        Err(anyhow::Error::msg("Disconnected"))
    }

    fn subscribe_hashblock(&mut self) -> Result<zmq::Socket> {
        let context = zmq::Context::new();
        let subscriber = context.socket(zmq::SUB).unwrap();

        subscriber.connect(&self.zmq_hashblock_url)?;
        subscriber.set_subscribe(b"hashblock")?;

        Ok(subscriber)
    }

    async fn subscribe(
        &mut self,
        thread: AppThread,
    ) -> Result<
        Option<(
            tokio::task::JoinHandle<Result<(), anyhow::Error>>,
            tokio::task::JoinHandle<Result<(), anyhow::Error>>,
        )>,
    > {
        let result = self.subscribe_hashblock();

        match result {
            Err(_) => Err(anyhow::Error::msg("Failed to subscribe")),
            Ok(sub) => {
                let res = self
                    .spawn_subscription_listener(sub, thread, self.state.clone())
                    .unwrap();
                Ok(Some(res))
            }
        }
    }
}

#[async_trait]
impl NodeProvider for BitcoinCore {
    fn new(config: AppConfig) -> Self {
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

        let zmq_hashblock_url = vec![
            "tcp://",
            &config.bitcoin_core.host,
            ":",
            &config.bitcoin_core.zmq_hashblock_port,
        ]
        .join("");

        Self {
            rpc_client: Arc::new(rpc),
            zmq_hashblock_url,
            state: NodeState::new(),
        }
    }

    async fn init(&mut self, thread: AppThread) -> Result<()> {
        let check_interval = time::Duration::from_millis(15 * 1000);

        tokio::select! {
            r = self.get_blockchain_info() => r.ok(),
            () = thread.token.cancelled() => {
                return Ok(());
            },
        };

        let mut sub_handlers = Box::new(tokio::select! {
            res = self.subscribe(thread.clone()) => res,
            () = thread.token.cancelled() => Ok(None),
        });

        loop {
            if thread.token.is_cancelled() {
                break;
            }

            if let Ok(Some(ref handlers)) = *sub_handlers {
                let (wait, subscribe) = handlers;
                if wait.is_finished() || subscribe.is_finished() {
                    sub_handlers = Box::new(tokio::select! {
                        res = self.subscribe(thread.clone()) => res,
                        () = thread.token.cancelled() => Ok(None),
                    })
                }
            }

            tokio::select! {
                r = self.get_blockchain_info() => r.ok(),
                () = thread.token.cancelled() => {
                    break;
                },
            };

            tokio::select! {
                () = tokio::time::sleep(check_interval) => {},
                () = thread.token.cancelled() => {
                    break;
                },
            };
        }

        Ok(())
    }

    fn get_state(&self) -> Arc<Mutex<NodeState>> {
        return self.state.clone();
    }
}
