use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::{env, error};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::config::AppConfig;
use crate::event::Event;
use crate::fees::providers::FeesBlockchainInfo;
use crate::fees::{spawn_fees_checker, FeesState};
use crate::node::{Node, NodeProvider, NodeState};
use crate::price::providers::coinbase::PriceCoinbase;
use crate::price::{spawn_price_checker, PriceCurrency, PriceState};

/// Application result type.
pub type AppResult<T> = std::result::Result<T, Box<dyn error::Error>>;

#[derive(Debug, Clone)]
pub struct AppThread {
    pub sender: mpsc::UnboundedSender<Event>,
    pub tracker: TaskTracker,
    pub token: CancellationToken,
}

impl AppThread {
    pub fn new(sender: mpsc::UnboundedSender<Event>) -> Self {
        Self {
            sender,
            tracker: TaskTracker::new(),
            token: CancellationToken::new(),
        }
    }
}

pub struct AppState {
    pub counter: u8,
    pub price: PriceState,
    pub fees: FeesState,
    pub node: Option<Arc<Mutex<NodeState>>>,
}

pub struct App {
    pub node: Node,
    pub thread: AppThread,
    pub config: AppConfig,
    pub state: AppState,
    pub running: bool,
}

impl App {
    pub fn new(thread: AppThread) -> Self {
        let (args, argv) = argmap::parse(env::args());
        let config = AppConfig::new(args, argv).unwrap();
        let cloned_thread = thread.clone();
        Self {
            running: true,
            config,
            thread,
            node: Node::new(cloned_thread),
            state: AppState {
                counter: 0,
                price: PriceState::new(),
                fees: FeesState::new(),
                node: Some(NodeState::new()),
            },
        }
    }

    pub fn init_node(&mut self, provider: Box<dyn NodeProvider + Send>) {
        self.state.node = Some(provider.get_state());
        self.node.init(provider);
    }

    pub fn init_price(&mut self) {
        spawn_price_checker::<PriceCoinbase>(
            self.thread.clone(),
            PriceCurrency::from_str(&self.config.price.currency).unwrap(),
        );

        spawn_fees_checker::<FeesBlockchainInfo>(self.thread.clone());
    }

    pub fn tick(&mut self) {}

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn increment_counter(&mut self) {
        if let Some(res) = self.state.counter.checked_add(1) {
            self.state.counter = res;
        }
    }

    pub fn decrement_counter(&mut self) {
        if let Some(res) = self.state.counter.checked_sub(1) {
            self.state.counter = res;
        }
    }

    pub fn handle_price_update(&mut self, state: PriceState) {
        self.state.price = state;
    }

    pub fn handle_fee_update(&mut self, state: FeesState) {
        self.state.fees = state;
    }

    pub fn handle_key_events(&mut self, key_event: KeyEvent) -> AppResult<()> {
        // self.reset_last_hash_time();
        match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.quit();
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                if key_event.modifiers == KeyModifiers::CONTROL {
                    self.quit();
                }
            }
            KeyCode::Right => {
                self.increment_counter();
            }
            KeyCode::Left => {
                self.decrement_counter();
            }
            KeyCode::Char(' ') => {}
            _ => {}
        }
        Ok(())
    }
}
