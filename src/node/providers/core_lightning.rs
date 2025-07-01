use anyhow::Result;
use async_trait::async_trait;
use cln_rpc::model::requests::GetinfoRequest;
use cln_rpc::ClnRpc;
use std::sync::{Arc, Mutex};
use tokio::time;

use crate::{
    app::AppThread,
    config::AppConfig,
    node::{NodeProvider, NodeState, NodeStatus},
};

#[derive(Clone)]
pub struct CoreLightning {
    rpc_socket: String,
    rpc_client: Arc<tokio::sync::Mutex<Option<ClnRpc>>>,
    state: Arc<Mutex<NodeState>>,
}

impl CoreLightning {
    async fn get_node_info(&self) -> Result<()> {
        let mut client_guard = self.rpc_client.lock().await;
        let client = client_guard
            .as_mut()
            .expect("CLN RPC client not initialized");

        let response = client.call_typed(&GetinfoRequest {}).await?;

        let mut state = self.state.lock().unwrap();

        state.status = NodeStatus::Online;
        state.height = response.blockheight as u64;
        state.last_hash = "N/A".to_string();
        state.message = format!(
            "Alias: {}",
            response.alias.unwrap_or_else(|| "Unknown".to_string())
        );

        *state
            .services
            .entry("RPC".to_string())
            .or_insert(NodeStatus::Online) = NodeStatus::Online;

        Ok(())
    }

    async fn check_node_status(&self) -> Result<()> {
        match self.get_node_info().await {
            Ok(_) => Ok(()),
            Err(e) => {
                let mut state = self.state.lock().unwrap();
                state.status = NodeStatus::Offline;
                *state
                    .services
                    .entry("RPC".to_string())
                    .or_insert(NodeStatus::Offline) = NodeStatus::Offline;
                Err(e)
            }
        }
    }
}

#[async_trait]
impl NodeProvider for CoreLightning {
    fn new(config: &AppConfig) -> Self {
let rpc_socket = config.core_lightning.rpc_socket_path.clone();

        let state = NodeState::new();
        {
            let mut locked_state = state.lock().unwrap();
            locked_state
                .services
                .insert("RPC".to_string(), NodeStatus::Offline);
        }

        Self {
            rpc_socket,
            rpc_client: Arc::new(tokio::sync::Mutex::new(None)),
            state,
        }
    }

    async fn init(&mut self, thread: AppThread) -> Result<()> {
        // Initialize the CLN RPC client asynchronously
        let client: ClnRpc = ClnRpc::new(&self.rpc_socket).await.unwrap();
        {
            let mut locked_client = self.rpc_client.lock().await;
            *locked_client = Some(client);
        }

        let check_interval = time::Duration::from_secs(15);

        loop {
            if thread.token.is_cancelled() {
                break;
            }

            let _ = self.check_node_status().await;

            tokio::time::sleep(check_interval).await;
        }

        Ok(())
    }

    fn get_state(&self) -> Arc<Mutex<NodeState>> {
        self.state.clone()
    }
}
