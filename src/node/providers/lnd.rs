use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use tokio::time::{self, Duration};

use crate::{
    app::AppThread,
    config::AppConfig,
    node::{NodeProvider, NodeState, NodeStatus},
};

#[derive(Debug, Deserialize)]
struct GetInfoResponse {
    pub block_height: u64,
    pub alias: String,
}

#[derive(Clone)]
pub struct LndNode {
    address: String,
    macaroon: String,
    client: Arc<Client>,
    state: Arc<Mutex<NodeState>>,
}

impl LndNode {
    async fn get_node_info(&self) -> Result<()> {
        let url = format!("{}/v1/getinfo", self.address);

        let response_result = self
            .client
            .get(&url)
            .header("Grpc-Metadata-macaroon", &self.macaroon)
            .send()
            .await;

        match response_result {
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() {
                    let mut state = self.state.lock().unwrap();
                    state.message = format!("LND REST error: HTTP {}", status);
                    return Err(anyhow::anyhow!("LND REST non-200: {}", status));
                }

                let info = resp.json::<GetInfoResponse>().await?;

                let mut state = self.state.lock().unwrap();
                state.message = "".to_string();
                state.status = NodeStatus::Online;
                state.height = info.block_height;
                state.last_hash = "N/A".to_string();
                state.alias = info.alias;

                *state
                    .services
                    .entry("REST".to_string())
                    .or_insert(NodeStatus::Online) = NodeStatus::Online;

                Ok(())
            }
            Err(e) => {
                let mut state = self.state.lock().unwrap();
                state.message = format!("LND REST req error: {:?}", e);
                Err(anyhow::anyhow!("Request error: {:?}", e))
            }
        }
    }

    async fn check_node_status(&self) -> Result<()> {
        match self.get_node_info().await {
            Ok(_) => Ok(()),
            Err(e) => {
                let mut state = self.state.lock().unwrap();
                state.status = NodeStatus::Offline;
                *state
                    .services
                    .entry("REST".to_string())
                    .or_insert(NodeStatus::Offline) = NodeStatus::Offline;
                Err(e)
            }
        }
    }
}

#[async_trait]
impl NodeProvider for LndNode {
    fn new(config: &AppConfig) -> Self {
        let state = NodeState::new();
        {
            let mut locked_state = state.lock().unwrap();
            
            locked_state.title = "LND".to_string();
            locked_state.host = config.lnd.rest_address.to_string();
            locked_state.message = "Initializing LND REST...".to_string();

            locked_state
                .services
                .insert("REST".to_string(), NodeStatus::Offline);
        }

        let address = config.lnd.rest_address.clone();
        let macaroon = config.lnd.macaroon_hex.clone();

        let client = Client::builder()
            .danger_accept_invalid_certs(true) // For self-signed TLS from LND
            .build()
            .unwrap();

        Self {
            address,
            macaroon,
            client: Arc::new(client),
            state,
        }
    }

    async fn init(&mut self, thread: AppThread) -> Result<()> {
        let check_interval = Duration::from_secs(15);

        loop {
            if thread.token.is_cancelled() {
                break;
            }

            let _ = self.check_node_status().await;

            time::sleep(check_interval).await;
        }

        Ok(())
    }

    fn get_state(&self) -> Arc<Mutex<NodeState>> {
        self.state.clone()
    }
}
