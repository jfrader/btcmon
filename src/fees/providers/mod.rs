use async_trait::async_trait;
use serde::Deserialize;

use super::{FeeResult, FeeServiceProvider};
pub struct FeesBlockchainInfo;

#[derive(Debug, Deserialize)]
struct BlockchainInfoResponseLimits {
    min: u32,
    // max: u32,
}

#[derive(Debug, Deserialize)]
struct BlockchainInfoResponse {
    limits: BlockchainInfoResponseLimits,
    regular: u32,
    priority: u32,
}

#[async_trait]
impl FeeServiceProvider for FeesBlockchainInfo {
    fn new() -> Self {
        Self::default()
    }

    async fn fetch_current_fees(&mut self) -> Result<FeeResult, Box<dyn std::error::Error>> {
        let client = reqwest::Client::builder().build().unwrap();

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());

        let request = client
            .get("https://api.blockchain.info/mempool/fees".to_string())
            .headers(headers)
            .send()
            .await;

        if let Err(e) = request {
            return Err(e.into());
        }

        let json = request.unwrap().json::<BlockchainInfoResponse>().await;

        if let Err(e) = json {
            return Err(e.into());
        }

        let body = json.unwrap();

        Ok(FeeResult {
            high: format!("{}", body.priority),
            medium: format!("{}", body.regular),
            low: format!("{}", body.limits.min),
        })
    }
}

impl Default for FeesBlockchainInfo {
    fn default() -> Self {
        Self
    }
}
