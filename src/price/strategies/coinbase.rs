use crate::price::price::{PriceResult, PriceStrategy, PriceTickerPair};
use async_trait::async_trait;
use serde::Deserialize;
pub struct CoinbasePrice;

#[derive(Debug, Deserialize)]
struct CoinbasePriceResponse {
    price: String,
}

#[async_trait]
impl PriceStrategy for CoinbasePrice {
    fn new() -> Self {
        Self::default()
    }

    async fn fetch_current_price(
        &mut self,
        _pair: &PriceTickerPair,
    ) -> Result<PriceResult, Box<dyn std::error::Error>> {
        let client = reqwest::Client::builder().build().unwrap();

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());

        let request = client
            .get("https://api.coinbase.com/api/v3/brokerage/market/products/BTC-USD")
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

impl Default for CoinbasePrice {
    fn default() -> Self {
        Self
    }
}
