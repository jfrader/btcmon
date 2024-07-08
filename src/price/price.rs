use async_trait::async_trait;
use std::fmt;
use tokio::sync::mpsc;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::event::Event;

pub enum PriceTickerPair {
    USDBTC,
}

#[derive(Debug, Clone, Copy)]
pub enum PriceCurrency {
    USD,
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
pub trait PriceStrategy {
    fn new() -> Self;
    async fn fetch_current_price(
        &mut self,
        pair: &PriceTickerPair,
    ) -> Result<PriceResult, Box<dyn std::error::Error>>;
}

#[derive(Debug, Clone, Copy)]
pub struct PriceState {
    pub currency: PriceCurrency,
    pub last_price_in_currency: Option<f64>,
}

pub struct PriceProvider<TProvider: PriceStrategy> {
    pub provider: TProvider,
    pub last_price_in_currency: Option<String>,
}

impl<TProvider: PriceStrategy> PriceProvider<TProvider> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<TProvider: PriceStrategy> Default for PriceProvider<TProvider> {
    fn default() -> Self {
        Self {
            provider: TProvider::new(),
            last_price_in_currency: None,
        }
    }
}

pub fn spawn_price_checker<T: PriceStrategy>(
    sender: mpsc::UnboundedSender<Event>,
    tracker: TaskTracker,
    token: CancellationToken,
) where
    T: Send,
{
    tracker.spawn(async move {
        tokio::select! {
            () = token.cancelled() => {}
            () = price_checker::<T>(PriceTickerPair::USDBTC, sender, token.clone()) => {}
        }
    });
}

async fn price_checker<T: PriceStrategy>(
    pair: PriceTickerPair,
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
            res = provider.fetch_current_price(&pair) => {
                let _ = match res {
                    Ok(res) => sender.send(Event::PriceUpdate(PriceState {
                        currency: PriceCurrency::USD,
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
