use bitcoin::Amount;
use bitcoincore_rpc::{
    json::{EstimateSmartFeeResult, GetBlockchainInfoResult},
    Auth, Client, RpcApi,
};
use futures::channel::mpsc;
use std::{
    fmt, io,
    sync::{Arc, Mutex},
    time,
};
use tokio::time::Instant;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use zeromq::{Socket, SocketRecv, SubSocket};

use crate::{app::App, config::Settings};

#[derive(Clone, Debug, PartialEq)]
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
pub struct EstimatedFee {
    pub fee: Amount,
    pub target: u8,
    pub received_target: u8,
}

#[derive(Clone, Debug)]
pub struct BitcoinState {
    pub status: EBitcoinNodeStatus,
    pub current_height: u64,
    pub header_height: u64,
    pub last_hash: String,
    pub last_hash_time: Option<Instant>,
    pub fees: Vec<EstimatedFee>,
}

impl Default for BitcoinState {
    fn default() -> Self {
        Self {
            status: EBitcoinNodeStatus::Offline,
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
}

impl BitcoinState {
    pub fn new() -> Self {
        Self::default()
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
                self.set_status(EBitcoinNodeStatus::Offline);
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
            EBitcoinNodeStatus::Synchronizing
        } else {
            EBitcoinNodeStatus::Online
        };

        let try_best_block_hash = rpc.get_best_block_hash();
        let best_block_hash = try_best_block_hash.unwrap();

        self.current_height = blockchain_info.blocks;
        self.header_height = blockchain_info.headers;
        self.push_block(best_block_hash.to_string(), false);
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

#[allow(unused_assignments)]
pub fn try_connect_to_node(
    config_provider: Settings,
    app: &mut App,
    tracker: TaskTracker,
    token: CancellationToken,
) {
    let mut status = EBitcoinNodeStatus::Online;

    {
        let state_locked = app.bitcoin_state.lock();
        status = state_locked.unwrap().status.clone();
    }

    match status {
        EBitcoinNodeStatus::Offline => Some(spawn_connect_to_node(
            config_provider,
            app.bitcoin_state.clone(),
            tracker,
            token,
        )),
        _ => None,
    };
}

pub fn spawn_connect_to_node(
    config: Settings,
    bitcoin_state: Arc<Mutex<BitcoinState>>,
    tracker: TaskTracker,
    token: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    {
        let mut state = bitcoin_state.lock().unwrap();
        state.set_status(EBitcoinNodeStatus::Connecting);
    }

    let subtasks_tracker = tracker.clone();
    let subtasks_token = token.clone();
    tracker.spawn(async move {
        tokio::select! {
            () = connect_to_node(config, bitcoin_state, subtasks_tracker, subtasks_token) => {},
            () = token.cancelled() => {},
        }
    })
}

async fn connect_to_node(
    config: Settings,
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
                let blocks_tracker = tracker.clone();
                let blocks_token = token.clone();
                spawn_blocks_receiver(bitcoin_state.clone(), socket, blocks_tracker, blocks_token);
                spawn_rpc_checker(bitcoin_state.clone(), rpc, rpc_tracker, rpc_token);
                break;
            } else {
                spawn_rpc_checker(bitcoin_state, rpc, rpc_tracker, rpc_token);
                break;
            };
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

        let mut status = EBitcoinNodeStatus::Online;

        {
            let state_locked = state.lock();
            status = state_locked.unwrap().status.clone();
        }

        if status == EBitcoinNodeStatus::Online {
            let receiver = tokio::select! {
                receiver = socket.recv() => Some(receiver),
                () = token.cancelled() => None,
            };
            if let Some(receiver) = receiver {
                let repl: zeromq::ZmqMessage = receiver.unwrap();
                let hash = hex::encode(repl.get(1).unwrap());
                let mut state_locked = state.lock().unwrap();
                if state_locked.status == EBitcoinNodeStatus::Online {
                    state_locked.push_block(hash.to_string(), true);
                    state_locked.increase_height();
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

        let mut status = EBitcoinNodeStatus::Online;

        {
            let state_locked = try_connection_state.lock();
            status = state_locked.unwrap().status.clone();
        }

        let is_connected = match status {
            EBitcoinNodeStatus::Connecting | EBitcoinNodeStatus::Offline => false,
            _ => true,
        };

        if is_connected {
            {
                let mut state = try_connection_state.lock().unwrap();
                let _ = state.try_fetch_blockchain_info(&rpc);
                state.estimate_fees(&rpc);
                drop(state);
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
