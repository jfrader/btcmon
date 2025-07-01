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
    pub alias: String,
    pub blockheight: u64,
}

#[derive(Clone)]
pub struct CoreLightning {
    rest_address: String,
    rune: String,
    client: Arc<Client>,
    state: Arc<Mutex<NodeState>>,
}

impl CoreLightning {
    async fn get_node_info(&self) -> Result<()> {
        let url = format!("{}/v1/getinfo", self.rest_address);

        let resp_result = self
            .client
            .post(&url)
            .header("Rune", &self.rune)
            .header("Content-Type", "application/json")
            .body("{}")
            .send()
            .await;

        match resp_result {
            Ok(resp) => {
                let status = resp.status();

                if !status.is_success() {
                    let mut state = self.state.lock().unwrap();
                    state.message = format!("CLN REST HTTP error: {}", status);
                    return Err(anyhow::anyhow!("CLN REST returned {}", status));
                }

                
                let info = resp.json::<GetInfoResponse>().await?;
                
                let mut state = self.state.lock().unwrap();
                state.message = "".to_string();
                state.status = NodeStatus::Online;
                state.height = info.blockheight;
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
                state.message = format!("CLN REST req error: {:?}", e);
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
impl NodeProvider for CoreLightning {
    fn new(config: &AppConfig) -> Self {
        let state = NodeState::new();
        {
            let mut locked_state = state.lock().unwrap();

            locked_state.title = "Core Lightning".to_string();
            locked_state.host = config.core_lightning.rest_address.to_string();
            locked_state.message = "Initializing CLN REST...".to_string();

            locked_state
                .services
                .insert("REST".to_string(), NodeStatus::Offline);
        }

        let rest_address = config.core_lightning.rest_address.clone();
        let rune = config.core_lightning.rest_rune.clone();

        let client = Client::builder()
            .danger_accept_invalid_certs(true) // For self-signed certs
            .build()
            .unwrap();

        Self {
            rest_address,
            rune,
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
