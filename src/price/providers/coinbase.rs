use crate::price::{PriceCurrency, PriceProvider, PriceResult};
use async_trait::async_trait;
use serde::Deserialize;
pub struct PriceCoinbase;

#[derive(Debug, Deserialize)]
struct CoinbasePriceResponse {
    price: String,
}

#[async_trait]
impl PriceProvider for PriceCoinbase {
    fn new() -> Self {
        Self
    }

    async fn fetch_current_price(
        &mut self,
        currency: &PriceCurrency,
    ) -> Result<PriceResult, Box<dyn std::error::Error>> {
        let client = reqwest::Client::builder().build()?;
        let response = client
            .get(format!(
                "https://api.coinbase.com/api/v3/brokerage/market/products/BTC-{currency}"
            ))
            .send()
            .await?
            .error_for_status()?;
        let body = response.json::<CoinbasePriceResponse>().await?;

        Ok(PriceResult {
            price_in_currency: body.price,
        })
    }
}

impl Default for PriceCoinbase {
    fn default() -> Self {
        Self
    }
}
