use anyhow::Result;
use async_trait::async_trait;
use std::fmt;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{app::AppThread, event::Event};

pub mod providers;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PriceCurrency {
    USD,
    EUR,
}

impl FromStr for PriceCurrency {
    type Err = anyhow::Error;
    fn from_str(input: &str) -> Result<PriceCurrency> {
        match input {
            "USD" => Ok(PriceCurrency::USD),
            "EUR" => Ok(PriceCurrency::EUR),
            _ => Err(anyhow::Error::msg("Currency not allowed")),
        }
    }
}

impl fmt::Display for PriceCurrency {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
pub struct PriceResult {
    pub price_in_currency: String,
}

#[async_trait]
pub trait PriceProvider {
    fn new() -> Self;
    async fn fetch_current_price(
        &mut self,
        currency: &PriceCurrency,
    ) -> Result<PriceResult, Box<dyn std::error::Error>>;
}

#[derive(Debug, Clone, Copy)]
pub struct PriceState {
    pub currency: PriceCurrency,
    pub last_price_in_currency: Option<f64>,
}

impl Default for PriceState {
    fn default() -> Self {
        Self {
            currency: PriceCurrency::USD,
            last_price_in_currency: None,
        }
    }
}

impl PriceState {
    pub fn new() -> Self {
        Self::default()
    }
}

pub struct Price<TProvider: PriceProvider> {
    pub provider: TProvider,
    pub last_price_in_currency: Option<String>,
}

impl<TProvider: PriceProvider> Price<TProvider> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<TProvider: PriceProvider> Default for Price<TProvider> {
    fn default() -> Self {
        Self {
            provider: TProvider::new(),
            last_price_in_currency: None,
        }
    }
}

pub fn spawn_price_checker<T: PriceProvider>(thread: AppThread, currency: PriceCurrency)
where
    T: Send,
{
    thread.tracker.spawn(async move {
        tokio::select! {
            () = thread.token.cancelled() => {}
            () = price_checker::<T>(currency, thread.sender, thread.token.clone()) => {}
        }
    });
}

async fn price_checker<T: PriceProvider>(
    currency: PriceCurrency,
    sender: mpsc::UnboundedSender<Event>,
    token: CancellationToken,
) {
    let mut provider = T::new();
    let interval = tokio::time::Duration::from_millis(30 * 1000);

    loop {
        if token.is_cancelled() {
            break;
        }
        tokio::select! {
            () = token.cancelled() => {}
            res = provider.fetch_current_price(&currency) => {
                let _ = match res {
                    Ok(res) => sender.send(Event::PriceUpdate(PriceState {
                        currency,
                        last_price_in_currency: Some(res.price_in_currency.parse::<f64>().unwrap()),
                    })),
                    Err(_) => Ok(()),
                };

            }
        }

        tokio::select! {
            () = token.cancelled() => {}
            () = tokio::time::sleep(interval) => {}
        }
    }
}
