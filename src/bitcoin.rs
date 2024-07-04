use bitcoincore_rpc::{json::GetBlockchainInfoResult, Auth, Client, RpcApi};
use futures::channel::mpsc;
use std::{
    fmt,
    sync::{Arc, Mutex},
    thread, time,
};
use zeromq::{Socket, SocketRecv};

#[derive(Clone, Debug)]
pub enum EBitcoinNodeStatus {
    Offline,
    Connecting,
    Online,
    BlocksBehind,
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
    pub block_hashes: Vec<String>,
    pub last_hash: String,
}

impl Default for BitcoinState {
    fn default() -> Self {
        Self {
            status: EBitcoinNodeStatus::Connecting,
            current_height: 0,
            header_height: 0,
            block_hashes: Vec::new(),
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
                    EBitcoinNodeStatus::BlocksBehind
                } else {
                    EBitcoinNodeStatus::Online
                };

                Ok(rpc.get_blockchain_info().unwrap())
            }
            Err(e) => {
                self.status = EBitcoinNodeStatus::Offline;
                Err(e)
            }
        }
    }

    pub fn connect(&mut self) -> Result<Client, bitcoincore_rpc::Error> {
        let rpc = Client::new(
            "127.0.0.1:18443",
            Auth::UserPass("polaruser".to_string(), "polarpass".to_string()),
        )
        .unwrap();

        let duration = time::Duration::from_millis(1200);
        thread::sleep(duration);

        let blockchain_info = self.check_rpc_connection(&rpc);

        match blockchain_info {
            Ok(blockchain_info) => {
                let try_best_block_hash = rpc.get_best_block_hash();
                let best_block_hash = try_best_block_hash.unwrap();

                self.current_height = blockchain_info.blocks;
                self.header_height = blockchain_info.headers;
                self.push_block(best_block_hash.to_string());
            }
            Err(_) => (),
        }

        Ok(rpc)
    }

    pub fn push_block(&mut self, hash: String) {
        self.block_hashes.push(hash.clone());
        self.last_hash = hash;
    }

    pub fn increase_height(&mut self) {
        if let Some(res) = self.current_height.checked_add(1) {
            self.current_height = res;
        }
    }
}

pub fn check_zmq_connection(monitor: &mut mpsc::Receiver<zeromq::SocketEvent>) -> i32 {
    match monitor.try_next() {
        Ok(Some(t)) => {
            dbg!(t);
            0
        }
        Ok(None) => 1,
        Err(_) => 2,
    }
}

pub async fn wait_for_blocks(state: Arc<Mutex<BitcoinState>>) {
    let mut socket = zeromq::SubSocket::new();
    socket
        .connect("tcp://127.0.0.1:28334")
        .await
        .expect("Failed to connect");

    socket
        .subscribe("hashblock")
        .await
        .expect("Failed to subscribe");

    loop {
        // dbg!(state.clone().lock().unwrap().block_hashes.last());
        let repl: zeromq::ZmqMessage = socket.recv().await.unwrap();
        // let event: String = String::from_utf8(repl.get(0).unwrap().to_vec()).unwrap();

        let hash = hex::encode(repl.get(1).unwrap());

        let mut unlocked_state = state.lock().unwrap();
        unlocked_state.push_block(hash.to_string());
        unlocked_state.increase_height();
    }
}
