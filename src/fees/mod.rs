use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{app::AppThread, event::Event};

pub mod providers;

#[derive(Debug, Clone)]
pub struct FeeResult {
    pub low: String,
    pub medium: String,
    pub high: String,
}

#[async_trait]
pub trait FeeServiceProvider {
    fn new() -> Self;
    async fn fetch_current_fees(&mut self) -> Result<FeeResult, Box<dyn std::error::Error>>;
}

#[derive(Debug, Clone)]
pub struct FeesState {
    pub result: FeeResult,
}

impl Default for FeesState {
    fn default() -> Self {
        Self {
            result: FeeResult {
                low: "-".to_string(),
                medium: "-".to_string(),
                high: "-".to_string(),
            },
        }
    }
}

impl FeesState {
    pub fn new() -> Self {
        Self::default()
    }
}

pub struct FeeService<TProvider: FeeServiceProvider> {
    pub provider: TProvider,
    pub result: Option<FeeResult>,
}

impl<TProvider: FeeServiceProvider> FeeService<TProvider> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<TProvider: FeeServiceProvider> Default for FeeService<TProvider> {
    fn default() -> Self {
        Self {
            provider: TProvider::new(),
            result: None,
        }
    }
}

pub fn spawn_fees_checker<T: FeeServiceProvider>(thread: AppThread)
where
    T: Send,
{
    thread.tracker.spawn(async move {
        tokio::select! {
            () = thread.token.cancelled() => {}
            () = fees_checker::<T>(thread.sender, thread.token.clone()) => {}
        }
    });
}

async fn fees_checker<T: FeeServiceProvider>(
    sender: mpsc::UnboundedSender<Event>,
    token: CancellationToken,
) {
    let mut provider = T::new();
    let interval = tokio::time::Duration::from_millis(20 * 1000);

    loop {
        if token.is_cancelled() {
            break;
        }
        tokio::select! {
            () = token.cancelled() => {}
            res = provider.fetch_current_fees() => {
                let _ = match res {
                    Ok(res) => sender.send(Event::FeeUpdate(FeesState {
                        result: FeeResult {
                            low: res.low,
                            medium: res.medium,
                            high: res.high,
                        }
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
