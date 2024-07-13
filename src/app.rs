use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::config::AppConfig;
use crate::event::Event;
use crate::node::node::{Node, NodeProvider, NodeState};
use crate::price::price::{spawn_price_checker, PriceCurrency, PriceState};
use crate::price::providers::coinbase::PriceCoinbase;
use std::{env, error};

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
    pub node: Option<Arc<Mutex<NodeState>>>,
}

pub struct App {
    pub node: Node,
    pub node_handler: Option<JoinHandle<()>>,
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
            node_handler: None,
            state: AppState {
                node: None,
                counter: 0,
                price: PriceState {
                    currency: PriceCurrency::USD,
                    last_price_in_currency: None,
                },
            },
        }
    }

    pub fn init_node(&mut self, provider: Box<dyn NodeProvider + Send>) {
        if let Some(handler) = &self.node_handler {
            handler.abort();
            self.thread.token.cancel();
            self.thread.tracker.close();
        }
        
        self.state.node = Some(provider.get_state());
        self.node_handler = Some(self.node.init(provider));
    }

    pub fn init_price(&mut self) {
        spawn_price_checker::<PriceCoinbase>(
            self.thread.sender.clone(),
            self.thread.tracker.clone(),
            self.thread.token.clone(),
        );
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
