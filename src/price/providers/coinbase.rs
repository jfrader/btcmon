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
        Self::default()
    }

    async fn fetch_current_price(
        &mut self,
        currency: &PriceCurrency,
    ) -> Result<PriceResult, Box<dyn std::error::Error>> {
        let client = reqwest::Client::builder().build().unwrap();

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());

        let request = client
            .get(
                vec![
                    "https://api.coinbase.com/api/v3/brokerage/market/products/BTC",
                    &currency.to_string(),
                ]
                .join("-"),
            )
            .headers(headers)
            .send()
            .await;

        if let Err(e) = request {
            return Err(Box::new(e));
        }

        let json = request.unwrap().json::<CoinbasePriceResponse>().await;

        if let Err(e) = json {
            return Err(Box::new(e));
        }

        let body = json.unwrap();

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
