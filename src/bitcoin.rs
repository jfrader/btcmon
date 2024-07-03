use bitcoincore_rpc::{Auth, Client, RpcApi};
use zeromq::{Socket, SocketRecv};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct BitcoinState {
    pub block_hashes: Vec<String>,
}

impl Default for BitcoinState {
    fn default() -> Self {
        Self {
            block_hashes: Vec::new(),
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

        _self.block_hashes.push(best_block_hash.to_string());

        _self
    }

    pub fn push_block(&mut self, hash: String) {
        self.block_hashes.push(hash);
    }
}

pub async fn get_blocks(state: Arc<Mutex<BitcoinState>>) {
    let rpc = Client::new(
        "127.0.0.1:18443",
        Auth::UserPass("polaruser".to_string(), "polarpass".to_string()),
    )
    .unwrap();

    let best_block_hash = rpc.get_best_block_hash().unwrap();

    dbg!(best_block_hash);

    let mut socket = zeromq::SubSocket::new();
    socket
        .connect("tcp://127.0.0.1:28334")
        .await
        .expect("Failed to connect");

    // println!("Connected");

    socket.subscribe("hashblock").await.unwrap();

    loop {
        // dbg!(state.clone().lock().unwrap().block_hashes.last());
        let repl: zeromq::ZmqMessage = socket.recv().await.unwrap();
        // let event: String = String::from_utf8(repl.get(0).unwrap().to_vec()).unwrap();

        let hash = hex::encode(repl.get(1).unwrap());

        state.lock().unwrap().push_block(hash.to_string());
    }
}
