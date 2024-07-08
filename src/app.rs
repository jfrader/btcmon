use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::bitcoin::{try_connect_to_node, BitcoinState};
use crate::config::Settings;
use crate::event::Event;
use crate::price::price::{spawn_price_checker, PriceCurrency, PriceState};
use crate::price::strategies::coinbase::CoinbasePrice;
use std::sync::{Arc, Mutex};
use std::{env, error};

/// Application result type.
pub type AppResult<T> = std::result::Result<T, Box<dyn error::Error>>;

pub struct App {
    pub running: bool,
    pub counter: u8,
    pub config: Settings,
    pub sender: mpsc::UnboundedSender<Event>,
    pub thread_tracker: TaskTracker,
    pub thread_token: CancellationToken,
    pub bitcoin_state: Arc<Mutex<BitcoinState>>,
    pub price_state: PriceState,
}

impl App {
    /// Constructs a new instance of [`App`].
    pub fn new(
        sender: mpsc::UnboundedSender<Event>,
    ) -> Self {
        let (args, argv) = argmap::parse(env::args());
        let config = Settings::new(args, argv).unwrap();
        Self {
            running: true,
            counter: 0,
            config,
            sender,
            thread_tracker: TaskTracker::new(),
            thread_token: CancellationToken::new(),
            bitcoin_state: Arc::new(Mutex::new(BitcoinState::new())),
            price_state: PriceState {
                currency: PriceCurrency::USD,
                last_price_in_currency: None,
            },
        }
    }

    pub fn init_price(&mut self) {
        spawn_price_checker::<CoinbasePrice>(
            self.sender.clone(),
            self.thread_tracker.clone(),
            self.thread_token.clone(),
        );
    }

    pub fn init_bitcoin(&mut self) {
        try_connect_to_node(
            self.config.clone(),
            self,
            self.thread_tracker.clone(),
            self.thread_token.clone(),
        );
    }

    /// Handles the tick event of the terminal.
    pub fn tick(&mut self) {}

    /// Set running to false to quit the application.
    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn reset_last_hash_time(&mut self) {
        self.bitcoin_state.lock().unwrap().last_hash_time = None;
    }

    pub fn increment_counter(&mut self) {
        if let Some(res) = self.counter.checked_add(1) {
            self.counter = res;
        }
    }

    pub fn handle_price_update(&mut self, state: PriceState) {
        self.price_state = state;
    }

    pub fn decrement_counter(&mut self) {
        if let Some(res) = self.counter.checked_sub(1) {
            self.counter = res;
        }
    }
}
