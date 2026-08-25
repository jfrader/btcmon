use anyhow::Result;
use async_trait::async_trait;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;
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
            _ => Err(anyhow::Error::msg("Currency not supported")),
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

#[derive(Debug, Clone)]
pub struct PriceState {
    pub currency: PriceCurrency,
    pub last_price_in_currency: Option<f64>,
    pub last_error: Option<String>,
    pub last_ok_at: Option<tokio::time::Instant>,
}

impl Default for PriceState {
    fn default() -> Self {
        Self {
            currency: PriceCurrency::USD,
            last_price_in_currency: None,
            last_error: None,
            last_ok_at: None,
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

pub fn spawn_price_checker<T: PriceProvider + Send>(thread: AppThread, currency: PriceCurrency) {
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
            res = tokio::time::timeout(Duration::from_secs(15), provider.fetch_current_price(&currency)) => {
                let update = match res {
                    Err(_) => PriceState {
                        currency,
                        last_price_in_currency: None,
                        last_error: Some("timed out".to_string()),
                        last_ok_at: None,
                    },
                    Ok(Ok(res)) => match res.price_in_currency.parse::<f64>() {
                        Ok(price) => PriceState {
                            currency,
                            last_price_in_currency: Some(price),
                            last_error: None,
                            last_ok_at: Some(tokio::time::Instant::now()),
                        },
                        Err(error) => PriceState {
                            currency,
                            last_price_in_currency: None,
                            last_error: Some(format!("bad price: {error}")),
                            last_ok_at: None,
                        },
                    },
                    Ok(Err(error)) => PriceState {
                        currency,
                        last_price_in_currency: None,
                        last_error: Some(short_error(&*error)),
                        last_ok_at: None,
                    },
                };
                let _ = sender.send(Event::PriceUpdate(update));
            }
        }

        tokio::select! {
            () = token.cancelled() => {}
            () = tokio::time::sleep(interval) => {}
        }
    }
}

fn short_error(error: &dyn std::error::Error) -> String {
    let text = error.to_string();
    let line = text.lines().next().unwrap_or("price request failed");
    if line.chars().count() <= 48 {
        line.to_string()
    } else {
        format!("{}~", line.chars().take(47).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::short_error;

    #[test]
    fn short_error_keeps_the_first_line() {
        let error = anyhow::anyhow!("connection reset\nmore detail");
        assert_eq!(short_error(error.as_ref()), "connection reset");
    }
}
