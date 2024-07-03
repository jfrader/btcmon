use bitcoincore_rpc::{Auth, Client, RpcApi};
use zeromq::{Socket, SocketRecv};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct BitcoinState {
    pub current_height: u64,
    pub header_height: u64,
    pub block_hashes: Vec<String>,
    pub last_hash: String,
}

impl Default for BitcoinState {
    fn default() -> Self {
        Self {
            current_height: 0,
            header_height: 0,
            block_hashes: Vec::new(),
            last_hash: String::new(),
        }
    }
}

impl BitcoinState {
    pub fn new() -> Self {
        let mut _self = Self::default();

        let rpc = Client::new(
            "127.0.0.1:18443",
            Auth::UserPass("polaruser".to_string(), "polarpass".to_string()),
        )
        .unwrap();
    
        let best_block_hash = rpc.get_best_block_hash().unwrap();
        let blockchain_info = rpc.get_blockchain_info().unwrap();

        _self.current_height = blockchain_info.blocks;
        _self.header_height = blockchain_info.headers;
        _self.push_block(best_block_hash.to_string());

        _self
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

pub async fn get_blocks(state: Arc<Mutex<BitcoinState>>) {
    let mut socket = zeromq::SubSocket::new();
    socket
        .connect("tcp://127.0.0.1:28334")
        .await
        .expect("Failed to connect");

    socket.subscribe("hashblock").await.expect("Failed to subscribe");

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
