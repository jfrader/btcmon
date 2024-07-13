use bitcoin::Amount;
use bitcoincore_rpc::{
    json::{EstimateSmartFeeResult, GetBlockchainInfoResult},
    Auth, Client, RpcApi,
};
use std::{
    fmt, io,
    sync::{Arc, Mutex},
    time,
};
use tokio::{sync::mpsc, time::Instant};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use zeromq::{Socket, SocketRecv, SubSocket};

use crate::{app::App, config::AppConfig, event::Event, node::providers::bitcoin_core::BitcoinCore};

#[derive(Clone, Debug, PartialEq)]
pub enum BitcoinCoreStatus {
    Offline,
    Connecting,
    Online,
    Synchronizing,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ZmqStatus {
    Offline,
    Connecting,
    Online,
}

impl fmt::Display for BitcoinCoreStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl fmt::Display for ZmqStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Clone, Debug)]
pub struct EstimatedFee {
    pub fee: Amount,
    pub target: u8,
    pub received_target: u8,
}

#[derive(Clone, Debug)]
pub struct BitcoinState {
    pub sender: mpsc::UnboundedSender<Event>,
    pub status: BitcoinCoreStatus,
    pub zmq_status: ZmqStatus,
    pub current_height: u64,
    pub header_height: u64,
    pub last_hash: String,
    pub last_hash_time: Option<Instant>,
    pub fees: Vec<EstimatedFee>,
}

impl BitcoinState {
    pub fn new(sender: mpsc::UnboundedSender<Event>) -> Self {
        Self {
            sender,
            status: BitcoinCoreStatus::Offline,
            zmq_status: ZmqStatus::Offline,
            current_height: 0,
            header_height: 0,
            last_hash: String::new(),
            last_hash_time: None,
            fees: vec![
                EstimatedFee {
                    fee: Amount::ZERO,
                    target: 1,
                    received_target: 1,
                },
                EstimatedFee {
                    fee: Amount::ZERO,
                    target: 3,
                    received_target: 3,
                },
                EstimatedFee {
                    fee: Amount::ZERO,
                    target: 10,
                    received_target: 10,
                },
            ],
        }
    }

    pub fn try_fetch_blockchain_info(
        &mut self,
        rpc: &Client,
    ) -> Result<GetBlockchainInfoResult, bitcoincore_rpc::Error> {
        match rpc.get_blockchain_info() {
            Ok(blockchain_info) => {
                self.update_blockchain_info(rpc, &blockchain_info);
                Ok(blockchain_info)
            }
            Err(e) => {
                self.zmq_status = ZmqStatus::Offline;
                self.set_status(BitcoinCoreStatus::Offline);
                Err(e)
            }
        }
    }

    pub fn set_estimated_fee(&mut self, index: usize, target: u8, result: EstimateSmartFeeResult) {
        if let (Some(fee), blocks) = (result.fee_rate, result.blocks) {
            self.fees[index] = EstimatedFee {
                fee,
                target,
                received_target: i64::try_into(blocks).unwrap_or(1),
            }
        }
    }

    pub fn estimate_fees(&mut self, rpc: &Client) {
        for (i, fee) in self.fees.clone().iter().enumerate() {
            match rpc.estimate_smart_fee(fee.target.try_into().unwrap_or(0), None) {
                Ok(estimation) => self.set_estimated_fee(i.into(), fee.target, estimation),
                _ => (),
            };
        }
    }

    pub fn update_blockchain_info(
        &mut self,
        rpc: &Client,
        blockchain_info: &GetBlockchainInfoResult,
    ) {
        self.status = if blockchain_info.blocks < blockchain_info.headers {
            BitcoinCoreStatus::Synchronizing
        } else {
            BitcoinCoreStatus::Online
        };

        let try_best_block_hash = rpc.get_best_block_hash();
        let best_block_hash = try_best_block_hash.unwrap();

        self.current_height = blockchain_info.blocks;
        self.header_height = blockchain_info.headers;
        self.push_block(best_block_hash.to_string(), false);
    }

    pub fn check_zmq_connection(
        &mut self,
        _monitor: &mut mpsc::Receiver<zeromq::SocketEvent>,
    ) -> i32 {
        todo!();
    }

    pub fn set_status(&mut self, status: BitcoinCoreStatus) {
        self.status = status;
    }

    pub fn push_block(&mut self, hash: String, notify: bool) {
        self.last_hash = hash;
        if notify {
            self.last_hash_time = Some(Instant::now());
        }
    }

    pub fn increase_height(&mut self) {
        if let Some(res) = self.current_height.checked_add(1) {
            self.current_height = res;
        }
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
        socket.connect(vec!["tcp://", host, ":", hashblock_port].join("").as_str()),
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

pub fn spawn_connect_to_node(
    app: &mut App<BitcoinCore>,
    tracker: TaskTracker,
    token: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    {
        let mut state = app.state.bitcoin_state.lock().unwrap();
        state.set_status(BitcoinCoreStatus::Connecting);
    }
    let subtasks_tracker = tracker.clone();
    let subtasks_token = token.clone();
    let bitcoin_state = app.state.bitcoin_state.clone();
    let config = app.config.clone();
    tracker.spawn(async move {
        tokio::select! {
            () = connect_to_node(config, bitcoin_state, subtasks_tracker, subtasks_token) => {},
            () = token.cancelled() => {},
        }
    })
}

async fn connect_to_node(
    config: AppConfig,
    bitcoin_state: Arc<Mutex<BitcoinState>>,
    tracker: TaskTracker,
    token: CancellationToken,
) {
    let connecting_interval = time::Duration::from_millis(3 * 1000);
    let sleep_interval = time::Duration::from_millis(15 * 1000);
    let mut retries: u8 = 0;

    loop {
        if token.is_cancelled() {
            break;
        }
        if retries > 3 {
            retries = 0;
            tokio::select! {
                () = tokio::time::sleep(sleep_interval) => {},
                () = token.cancelled() => {},
            }
        }
        retries = retries + 1;

        let result = tokio::select! {
            res = connect_rpc(
                &config.bitcoin_core.host,
                &config.bitcoin_core.rpc_port,
                &config.bitcoin_core.rpc_user,
                &config.bitcoin_core.rpc_password,
            ) => res,
            () = token.cancelled() => Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Aborting undergoing connection to Bitcoin RPC",
            )),
        };

        if let Ok((rpc, blockchain_info)) = result {
            {
                let connect_state = bitcoin_state.clone();
                let mut connect_state_locked = connect_state.lock().unwrap();
                connect_state_locked.update_blockchain_info(&rpc, &blockchain_info);
            }

            let socket = tokio::select! {
                res = connect_zmq(
                    &config.bitcoin_core.host,
                    &config.bitcoin_core.zmq_hashblock_port,
                ) => res,
                () = token.cancelled() => Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Aborting undergoing connection to Bitcoin ZMQ",
                ))
            };

            let rpc_tracker = tracker.clone();
            let rpc_token = token.clone();
            if let Ok(socket) = socket {
                {
                    let connect_state = bitcoin_state.clone();
                    let mut connect_state_locked = connect_state.lock().unwrap();
                    connect_state_locked.zmq_status = ZmqStatus::Online;
                }
                let blocks_tracker = tracker.clone();
                let blocks_token = token.clone();
                spawn_blocks_receiver(bitcoin_state.clone(), socket, blocks_tracker, blocks_token);
            };

            spawn_rpc_checker(bitcoin_state, rpc, rpc_tracker, rpc_token);
            break;
        };

        tokio::select! {
            () = tokio::time::sleep(connecting_interval) => {},
            () = token.cancelled() => {},
        }
    }
}

fn spawn_blocks_receiver(
    bitcoin_state: Arc<Mutex<BitcoinState>>,
    socket: SubSocket,
    tracker: TaskTracker,
    token: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    let connect_state = bitcoin_state.clone();
    tracker.spawn(async move {
        tokio::select! {
            () = wait_for_blocks(socket, connect_state, token.clone()) => {},
            () = token.cancelled() => {},
        }
    })
}

#[allow(unused_assignments)]
async fn wait_for_blocks(
    mut socket: zeromq::SubSocket,
    state: Arc<Mutex<BitcoinState>>,
    token: CancellationToken,
) {
    loop {
        if token.is_cancelled() {
            break;
        }

        let mut status = BitcoinCoreStatus::Online;

        {
            let state_locked = state.lock();
            status = state_locked.unwrap().status.clone();
        }

        if status == BitcoinCoreStatus::Online {
            let result = tokio::select! {
                receiver = socket.recv() => receiver,
                () = token.cancelled() => Err(zeromq::ZmqError::NoMessage),
            };

            match result {
                Ok(repl) => {
                    let decoded_hash = repl.get(1).unwrap();
                    let hash = hex::encode(decoded_hash);
                    let mut state_locked = state.lock().unwrap();
                    if state_locked.status == BitcoinCoreStatus::Online {
                        state_locked.push_block(hash.to_string(), true);
                        state_locked.increase_height();
                    }
                }
                Err(_) => {
                    let mut state_locked = state.lock().unwrap();
                    state_locked.zmq_status = ZmqStatus::Offline;
                }
            }
        } else {
            let _ = socket.close();
            break;
        };
    }
}

fn spawn_rpc_checker(
    bitcoin_state: Arc<Mutex<BitcoinState>>,
    rpc: Client,
    tracker: TaskTracker,
    token: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tracker.spawn(async move {
        tokio::select! {
            () = rpc_checker(bitcoin_state, rpc, token.clone()) => {},
            () = token.cancelled() => {},
        }
    })
}

#[allow(unused_assignments)]
async fn rpc_checker(
    bitcoin_state: Arc<Mutex<BitcoinState>>,
    rpc: Client,
    token: CancellationToken,
) {
    let check_interval = time::Duration::from_millis(30 * 1000);
    let try_connection_state = bitcoin_state.clone();
    loop {
        if token.is_cancelled() {
            break;
        }

        let mut status = BitcoinCoreStatus::Online;

        {
            let state_locked = try_connection_state.lock();
            status = state_locked.unwrap().status.clone();
        }

        let is_connected = match status {
            BitcoinCoreStatus::Connecting | BitcoinCoreStatus::Offline => false,
            _ => true,
        };

        if is_connected {
            {
                let mut state = try_connection_state.lock().unwrap();
                let res = state.try_fetch_blockchain_info(&rpc);

                match res {
                    Err(_) => {
                        let _ = state.sender.send(Event::NodeLostConnection);
                    }
                    Ok(_) => {
                        state.estimate_fees(&rpc);
                    }
                }
            }
            tokio::select! {
                () = tokio::time::sleep(check_interval) => {},
                () = token.cancelled() => {},
            }
        } else {
            break;
        }
    }
}
