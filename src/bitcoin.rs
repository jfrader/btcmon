use bitcoincore_rpc::{json::GetBlockchainInfoResult, Auth, Client, RpcApi};
use futures::channel::mpsc;
use std::{
    fmt, io,
    sync::{Arc, Mutex},
    thread, time,
};
use zeromq::{Socket, SocketRecv};

use crate::config::Settings;

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
            status: EBitcoinNodeStatus::Offline,
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
                self.update_blockchain_info(rpc, &blockchain_info);
                Ok(blockchain_info)
            }
            Err(e) => {
                self.set_status(EBitcoinNodeStatus::Offline);
                Err(e)
            }
        }
    }

    pub fn update_blockchain_info(
        &mut self,
        rpc: &Client,
        blockchain_info: &GetBlockchainInfoResult,
    ) {
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

    pub fn set_status(&mut self, status: EBitcoinNodeStatus) {
        self.status = status;
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

            let mut state_locked = state.lock().unwrap();
            state_locked.push_block(hash.to_string());
            state_locked.increase_height();
        } else {
            let _ = socket.close();
            break;
        };
    }
}

pub async fn connect_rpc(
    host: &String,
    port: &String,
    user: &String,
    password: &String,
) -> Result<(Client, GetBlockchainInfoResult), io::Error> {
    let rpc = Client::new(
        vec![host.as_str(), port.as_str()].join(":").as_str(),
        Auth::UserPass(user.to_string(), password.to_string()),
    )
    .unwrap();

    // let duration = time::Duration::from_millis(1200);
    // thread::sleep(duration);

    match rpc.get_blockchain_info() {
        Ok(blockchain_info) => Ok((rpc, blockchain_info)),
        Err(_) => Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "Failed to connect to Bitcoin RPC",
        )),
    }
}

pub async fn connect_zmq(
    host: &String,
    hashblock_port: &String,
) -> Result<zeromq::SubSocket, io::Error> {
    let mut socket = zeromq::SubSocket::new();

    if let Err(_) = tokio::time::timeout(
        time::Duration::from_millis(5000),
        socket.connect(
            vec!["tcp://", host, ":", hashblock_port]
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

pub async fn try_connect_to_node(
    config_provider: Settings,
    bitcoin_state: Arc<Mutex<BitcoinState>>,
) -> Result<(), io::Error>
{
    let unlocked_bitcoin_state = bitcoin_state.lock().unwrap().status.clone();

    match unlocked_bitcoin_state {
        EBitcoinNodeStatus::Offline => {
            connect_to_node(config_provider, bitcoin_state.clone()).await
        }
        _ => Ok(()),
    }
}

pub async fn connect_to_node(
    config: Settings,
    bitcoin_state: Arc<Mutex<BitcoinState>>,
) -> Result<(), io::Error>
{
    {
        let mut state = bitcoin_state.lock().unwrap();
        state.set_status(EBitcoinNodeStatus::Connecting);
    }
    tokio::spawn(async move {
        let check_interval = time::Duration::from_millis(30 * 1000);
        let connecting_interval = time::Duration::from_millis(3 * 1000);
        let sleep_interval = time::Duration::from_millis(15 * 1000);
        let mut retries: u8 = 0;

        loop {
            if retries > 3 {
                retries = 0;
                thread::sleep(sleep_interval);
            }
            retries = retries + 1;

            let result = connect_rpc(
                &config.bitcoin_core.host,
                &config.bitcoin_core.rpc_port,
                &config.bitcoin_core.rpc_user,
                &config.bitcoin_core.rpc_password,
            )
            .await;

            if let Ok((rpc, blockchain_info)) = result {
                {
                    let connect_state = bitcoin_state.clone();
                    let mut connect_state_locked = connect_state.lock().unwrap();
                    connect_state_locked.update_blockchain_info(&rpc, &blockchain_info);
                }

                let socket = connect_zmq(
                    &config.bitcoin_core.host,
                    &config.bitcoin_core.zmq_hashblock_port,
                )
                .await;

                if let Ok(socket) = socket {
                    {
                        let connect_state = bitcoin_state.clone();
                        tokio::spawn(async move {
                            wait_for_blocks(socket, connect_state).await;
                        });
                    }
                    {
                        let try_connection_state = bitcoin_state.clone();
                        tokio::spawn(async move {
                            loop {
                                let is_connected = match try_connection_state.lock().unwrap().status
                                {
                                    EBitcoinNodeStatus::Connecting
                                    | EBitcoinNodeStatus::Offline => false,
                                    _ => true,
                                };

                                if is_connected {
                                    {
                                        let _ = try_connection_state
                                            .lock()
                                            .unwrap()
                                            .check_rpc_connection(&rpc);
                                    }
                                    thread::sleep(check_interval);
                                } else {
                                    break;
                                }
                            }
                        });
                    }
                    break;
                };
            };
            thread::sleep(connecting_interval);
        }
    });

    Ok(())
}
