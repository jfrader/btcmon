use crate::bitcoin::BitcoinState;
use std::error;
use std::sync::{Arc, Mutex};

/// Application result type.
pub type AppResult<T> = std::result::Result<T, Box<dyn error::Error>>;

/// Application.
#[derive(Debug)]
pub struct App {
    pub tick_rate: u16,
    pub running: bool,
    pub counter: u8,
    pub bitcoin_state: Arc<Mutex<BitcoinState>>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            tick_rate: 0,
            running: true,
            counter: 0,
            bitcoin_state: Arc::new(Mutex::new(BitcoinState::new())),
        }
    }
}

impl App {
    /// Constructs a new instance of [`App`].
    pub fn new(tick_rate: u16) -> Self {
        let mut _self = Self::default();
        _self.tick_rate = tick_rate;
        _self
    }

    /// Handles the tick event of the terminal.
    pub fn tick(&mut self) {}

    /// Set running to false to quit the application.
    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn increment_counter(&mut self) {
        if let Some(res) = self.counter.checked_add(1) {
            self.counter = res;
        }
    }

    pub fn decrement_counter(&mut self) {
        if let Some(res) = self.counter.checked_sub(1) {
            self.counter = res;
        }
    }
}
