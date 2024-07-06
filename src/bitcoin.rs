use bitcoincore_rpc::{json::GetBlockchainInfoResult, Auth, Client, RpcApi};
use futures::channel::mpsc;
use std::{
    fmt,
    io,
    sync::{Arc, Mutex},
    // thread,
    time,
};
use zeromq::{Socket, SocketRecv};

use crate::config::ConfigProvider;

#[derive(Clone, Debug)]
pub enum EBitcoinNodeStatus {
    Offline,
    Connecting,
    Online,
    Synchronizing,
}

impl fmt::Display for EBitcoinNodeStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Clone, Debug)]
pub struct BitcoinState {
    pub status: EBitcoinNodeStatus,
    pub current_height: u64,
    pub header_height: u64,
    pub last_hash: String,
}

impl Default for BitcoinState {
    fn default() -> Self {
        Self {
            status: EBitcoinNodeStatus::Connecting,
            current_height: 0,
            header_height: 0,
            last_hash: String::new(),
        }
    }
}

impl BitcoinState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check_rpc_connection(
        &mut self,
        rpc: &Client,
    ) -> Result<GetBlockchainInfoResult, bitcoincore_rpc::Error> {
        match rpc.get_blockchain_info() {
            Ok(blockchain_info) => {
                self.status = if blockchain_info.blocks < blockchain_info.headers {
                    EBitcoinNodeStatus::Synchronizing
                } else {
                    EBitcoinNodeStatus::Online
                };

                let try_best_block_hash = rpc.get_best_block_hash();
                let best_block_hash = try_best_block_hash.unwrap();

                self.current_height = blockchain_info.blocks;
                self.header_height = blockchain_info.headers;
                self.push_block(best_block_hash.to_string());

                Ok(blockchain_info)
            }
            Err(e) => {
                self.status = EBitcoinNodeStatus::Offline;
                Err(e)
            }
        }
    }

    pub async fn connect_rpc(
        &mut self,
        host: &String,
        port: &u16,
        user: &String,
        password: &String,
    ) -> Result<Client, io::Error> {
        self.status = EBitcoinNodeStatus::Connecting;

        let rpc = Client::new(
            vec![host.as_str(), &port.to_string()].join(":").as_str(),
            Auth::UserPass(user.to_string(), password.to_string()),
        )
        .unwrap();

        // let duration = time::Duration::from_millis(1200);
        // thread::sleep(duration);

        let blockchain_info = self.check_rpc_connection(&rpc);

        match blockchain_info {
            Ok(_) => Ok(rpc),
            Err(_) => Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                "Failed to connect to Bitcoin RPC",
            )),
        }
    }

    pub async fn connect_zmq(
        &mut self,
        host: &String,
        hashblock_port: &u16,
    ) -> Result<zeromq::SubSocket, io::Error> {
        let mut socket = zeromq::SubSocket::new();

        if let Err(_) = tokio::time::timeout(
            time::Duration::from_millis(5000),
            socket.connect(
                vec!["tcp://", host.as_str(), ":", &hashblock_port.to_string()]
                    .join("")
                    .as_str(),
            ),
        )
        .await
        {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                "Failed to connect to ZMQ hashblock",
            ));
        }

        if let Err(_) = tokio::time::timeout(
            time::Duration::from_millis(5000),
            socket.subscribe("hashblock"),
        )
        .await
        {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                "Failed to subscribe to ZMQ hashblock",
            ));
        }

        Ok(socket)
    }

    // @todo: implement this fn
    pub fn check_zmq_connection(
        &mut self,
        monitor: &mut mpsc::Receiver<zeromq::SocketEvent>,
    ) -> i32 {
        match monitor.try_next() {
            Ok(Some(t)) => {
                dbg!(t);
                0
            }
            Ok(None) => 1,
            Err(_) => 2,
        }
    }

    pub fn push_block(&mut self, hash: String) {
        self.last_hash = hash;
    }

    pub fn increase_height(&mut self) {
        if let Some(res) = self.current_height.checked_add(1) {
            self.current_height = res;
        }
    }
}

pub async fn wait_for_blocks(mut socket: zeromq::SubSocket, state: Arc<Mutex<BitcoinState>>) {
    loop {
        let is_connected = match state.lock().unwrap().status {
            EBitcoinNodeStatus::Connecting | EBitcoinNodeStatus::Offline => false,
            _ => true,
        };

        if is_connected {
            // dbg!(state.clone().lock().unwrap().block_hashes.last());
            let repl: zeromq::ZmqMessage = socket.recv().await.unwrap();
            // let event: String = String::from_utf8(repl.get(0).unwrap().to_vec()).unwrap();

            let hash = hex::encode(repl.get(1).unwrap());

            let mut unlocked_state = state.lock().unwrap();
            unlocked_state.push_block(hash.to_string());
            unlocked_state.increase_height();
        } else {
            let _ = socket.close();
            break;
        };
    }
}

pub async fn try_connect_to_node<T>(
    config_provider: T,
    bitcoin_state: Arc<Mutex<BitcoinState>>,
) -> Result<(), io::Error>
where
    T: ConfigProvider,
{
    let mut unlocked_bitcoin_state = bitcoin_state.lock().unwrap();
    match unlocked_bitcoin_state.status {
        EBitcoinNodeStatus::Connecting | EBitcoinNodeStatus::Offline => {
            let config = config_provider.get_config();
            let rpc = unlocked_bitcoin_state
                .connect_rpc(
                    &config.bitcoin_core_host,
                    &config.bitcoin_core_rpc_port,
                    &config.bitcoin_core_rpc_user,
                    &config.bitcoin_core_rpc_password,
                )
                .await?;
            let socket = unlocked_bitcoin_state
                .connect_zmq(
                    &config.bitcoin_core_host,
                    &config.bitcoin_core_zmq_hashblock_port,
                )
                .await?;

            let wait_blocks_state = bitcoin_state.clone();
            tokio::spawn(async move {
                wait_for_blocks(socket, wait_blocks_state).await;
            });

            let try_connection_state = bitcoin_state.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(time::Duration::from_millis(10000));

                loop {
                    let is_connected = match try_connection_state.lock().unwrap().status {
                        EBitcoinNodeStatus::Connecting | EBitcoinNodeStatus::Offline => false,
                        _ => true,
                    };

                    if is_connected {
                        {
                            let _ = try_connection_state
                                .lock()
                                .unwrap()
                                .check_rpc_connection(&rpc);
                        }
                        interval.tick().await;
                    } else {
                        break;
                    }
                }
            });

            Ok(())
        }
        _ => Ok(()),
    }
}
