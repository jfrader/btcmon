use anyhow::Result;
use async_trait::async_trait;
use bitcoincore_rpc::{json::GetBlockchainInfoResult, RpcApi};
use zmq::PollEvents;
use std::sync::{Arc, Mutex};
use tokio::time::Instant;
use tokio::{sync::mpsc, time};
use tokio_util::sync::CancellationToken;

use crate::{
    app::AppThread,
    config::AppConfig,
    node::node::{NodeProvider, NodeState, NodeStatus},
};

enum BitcoinCoreEvent {
    NewBlock(String),
}

#[derive(Clone)]
enum BitcoinCoreSocketStatus {
    Online,
    Offline,
}

#[derive(Clone)]
pub struct BitcoinCoreSocket {
    host: String,
    status: Arc<Mutex<BitcoinCoreSocketStatus>>,
}

#[derive(Clone)]
pub struct BitcoinCoreSockets {
    zmq_hashblock: BitcoinCoreSocket,
}

#[derive(Clone)]
pub struct BitcoinCore {
    rpc: Arc<bitcoincore_rpc::Client>,
    state: Arc<Mutex<NodeState>>,
    sockets: Arc<BitcoinCoreSockets>,
}

impl BitcoinCore {
    async fn get_blockchain_info(&mut self) -> Result<GetBlockchainInfoResult> {
        match self.rpc.get_blockchain_info() {
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

    fn subscribe_hashblock(&mut self) -> Result<zmq::Socket> {
        let context = zmq::Context::new();
        let subscriber = context.socket(zmq::SUB).unwrap();

        subscriber.connect(&self.sockets.zmq_hashblock.host)?;
        subscriber.set_subscribe(b"hashblock")?;

        Ok(subscriber)
    }

    async fn receive_hashblock(
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

    fn spawn_hashblock_receiver(
        &mut self,
        socket: zmq::Socket,
        thread: AppThread,
        state: Arc<Mutex<NodeState>>,
    ) -> Result<(
        tokio::task::JoinHandle<Result<(), anyhow::Error>>,
        tokio::task::JoinHandle<Result<(), anyhow::Error>>,
    )> {
        let thread_clone = thread.clone();
        let (tx, rx) = mpsc::unbounded_channel::<BitcoinCoreEvent>();

        let receive_handler = tokio::task::spawn(async move {
            BitcoinCore::receive_hashblock(thread_clone, state, rx).await
        });

        let wait_handler = tokio::task::spawn(async move {
            BitcoinCore::wait_for_blocks(socket, tx, thread.token.clone()).await
        });

        return Ok((wait_handler, receive_handler));
    }

    fn receive_block(
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

    async fn wait_for_blocks(
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
                    Self::receive_block(sub.expect("No socket!"))
                }) => r.unwrap(),
                () = token.cancelled() => Err(None),
            };

            match result {
                Ok((ret_socket, repl)) => {
                    let hash = hex::encode(repl.get(1).unwrap());
                    tx.send(BitcoinCoreEvent::NewBlock(hash.clone()))?;
                    sub = Some(ret_socket);
                }
                _ => {
                    sub = None;
                }
            };
        }

        Err(anyhow::Error::msg("Disconnected"))
    }

    async fn subscribe(
        &mut self,
        thread: AppThread,
        sockets: Arc<BitcoinCoreSockets>,
    ) -> Result<
        Option<(
            tokio::task::JoinHandle<Result<(), anyhow::Error>>,
            tokio::task::JoinHandle<Result<(), anyhow::Error>>,
        )>,
    > {
        {
            let lock = sockets.zmq_hashblock.status.lock().unwrap();
            let status = lock.clone();

            if let BitcoinCoreSocketStatus::Online = status {
                return Ok(None);
            }
        }

        let result = self.subscribe_hashblock();

        match result {
            Err(_) => {
                let mut lock = sockets.zmq_hashblock.status.lock().unwrap();
                *lock = BitcoinCoreSocketStatus::Offline;

                Err(anyhow::Error::msg("Failed to subscribe"))
            }
            Ok(sub) => {
                {
                    let mut lock = sockets.zmq_hashblock.status.lock().unwrap();
                    *lock = BitcoinCoreSocketStatus::Online;
                }

                let res = self
                    .spawn_hashblock_receiver(sub, thread, self.state.clone())
                    .unwrap();
                Ok(Some(res))
            }
        }
    }

    async fn poll(&mut self, token: CancellationToken) -> Option<GetBlockchainInfoResult> {
        tokio::select! {
            res = async {
                if let Ok(res) = self.get_blockchain_info().await {
                    Some(res)
                } else {
                    None
                }
            }  => res,
            () = token.cancelled() => None,
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

        let state = NodeState {
            status: NodeStatus::Offline,
            height: 0,
            headers: 0,
            last_hash: "".to_string(),
            last_hash_instant: None,
        };

        let host = vec![
            "tcp://",
            &config.bitcoin_core.host,
            ":",
            &config.bitcoin_core.zmq_hashblock_port,
        ]
        .join("");

        Self {
            state: Arc::new(Mutex::new(state)),
            rpc: Arc::new(rpc),
            sockets: Arc::new(BitcoinCoreSockets {
                zmq_hashblock: BitcoinCoreSocket {
                    host,
                    status: Arc::new(Mutex::new(BitcoinCoreSocketStatus::Offline)),
                },
            }),
        }
    }

    async fn init(&mut self, thread: AppThread) -> Result<()> {
        let check_interval = time::Duration::from_millis(15 * 1000);

        let sockets = self.sockets.clone();

        let mut sub_handlers = Box::new(tokio::select! {
            res = self.subscribe(thread.clone(), sockets) => res,
            () = thread.token.cancelled() => Ok(None),
        });

        loop {
            if let Ok(Some(ref handlers)) = *sub_handlers {
                let (wait, subscribe) = handlers;
                if wait.is_finished() || subscribe.is_finished() {
                    let sockets = self.sockets.clone();
                    sub_handlers = Box::new(tokio::select! {
                        res = self.subscribe(thread.clone(), sockets) => res,
                        () = thread.token.cancelled() => Ok(None),
                    })
                }
            }

            if thread.token.is_cancelled() {
                break;
            }

            tokio::select! {
                _ = self.poll(thread.token.clone()) => {},
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
