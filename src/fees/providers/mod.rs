use async_trait::async_trait;
use serde::Deserialize;
use std::time::Duration;

use super::{FeeResult, FeeServiceProvider};
pub struct FeesBlockchainInfo;

#[derive(Debug, Deserialize)]
struct BlockchainInfoResponse {
    // limits: BlockchainInfoResponseLimits,
    regular: u32,
    priority: u32,
}

#[async_trait]
impl FeeServiceProvider for FeesBlockchainInfo {
    fn new() -> Self {
        Self
    }

    async fn fetch_current_fees(&mut self) -> Result<FeeResult, Box<dyn std::error::Error>> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
        let response = client
            .get("https://api.blockchain.info/mempool/fees")
            .send()
            .await?
            .error_for_status()?;
        let body = response.json::<BlockchainInfoResponse>().await?;

        Ok(FeeResult {
            high: Some(format!("{}", body.priority)),
            medium: Some(format!("{}", body.regular)),
            low: None,
        })
    }
}

impl Default for FeesBlockchainInfo {
    fn default() -> Self {
        Self
    }
}
