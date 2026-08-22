use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{app::AppThread, event::Event};

pub mod providers;

#[derive(Debug, Clone)]
pub struct FeeResult {
    pub low: Option<String>,
    pub medium: Option<String>,
    pub high: Option<String>,
}

#[async_trait]
pub trait FeeServiceProvider {
    fn new() -> Self;
    async fn fetch_current_fees(&mut self) -> Result<FeeResult, Box<dyn std::error::Error>>;
}

#[derive(Debug, Clone)]
pub struct FeesState {
    pub result: FeeResult,
    pub last_error: Option<String>,
}

impl Default for FeesState {
    fn default() -> Self {
        Self {
            result: FeeResult {
                low: None,
                medium: None,
                high: None,
            },
            last_error: None,
        }
    }
}

impl FeeResult {
    pub fn is_empty(&self) -> bool {
        self.low.is_none() && self.medium.is_none() && self.high.is_none()
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

pub fn spawn_fees_checker<T: FeeServiceProvider + Send>(thread: AppThread) {
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
                let update = match res {
                    Ok(result) => FeesState {
                        result,
                        last_error: None,
                    },
                    Err(error) => FeesState {
                        result: FeeResult {
                            low: None,
                            medium: None,
                            high: None,
                        },
                        last_error: Some(short_error(&*error)),
                    },
                };
                let _ = sender.send(Event::FeeUpdate(update));

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
    let line = text.lines().next().unwrap_or("fee request failed");
    if line.chars().count() <= 48 {
        line.to_string()
    } else {
        format!("{}~", line.chars().take(47).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::{short_error, FeeResult};

    #[test]
    fn empty_fee_result_is_detected() {
        assert!(FeeResult {
            low: None,
            medium: None,
            high: None,
        }
        .is_empty());
        assert!(!FeeResult {
            low: None,
            medium: Some("12".to_string()),
            high: None,
        }
        .is_empty());
    }

    #[test]
    fn short_error_keeps_the_first_line() {
        let error = anyhow::anyhow!("timed out\ntrace");
        assert_eq!(short_error(error.as_ref()), "timed out");
    }
}
