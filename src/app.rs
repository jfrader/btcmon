// app.rs

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::time::{Instant, Duration};
use std::str::FromStr;
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
use crate::widget::{DynamicState, DynamicStatefulWidget};

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
    pub node: NodeState,
    pub widget: Box<dyn DynamicStatefulWidget>,
    pub widget_state: Box<dyn DynamicState>,
}

pub struct App {
    pub node: Node,
    pub thread: AppThread,
    pub config: AppConfig,
    pub state: AppState,
    pub running: bool,
}

impl App {
    pub fn new(thread: AppThread, widget: Box<dyn DynamicStatefulWidget>, widget_state: Box<dyn DynamicState>) -> Self {
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
                node: NodeState::new(),
                widget,
                widget_state,
            },
        }
    }

    pub fn init_node(&mut self, provider: Box<dyn NodeProvider + Send>) {
        self.node.init(provider);
    }

    pub fn init_price(&mut self) {
        spawn_price_checker::<PriceCoinbase>(
            self.thread.clone(),
            PriceCurrency::from_str(&self.config.price.currency).unwrap(),
        );
    }

    pub fn init_fees(&mut self) {
        spawn_fees_checker::<FeesBlockchainInfo>(self.thread.clone());
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let switch_interval = Duration::from_secs(3);
        let keys: Vec<_> = self.state.node.services.keys().cloned().collect();

        if !keys.is_empty() {
            let should_advance = match self.state.node.last_service_switch {
                Some(last) => now.duration_since(last) >= switch_interval,
                None => true,
            };

            if should_advance {
                let new_index = (self.state.node.service_display_index + 1) % keys.len();
                self.state
                    .node
                    .set_last_service_switch(Some(now), new_index);
            }
        }
    }

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

    pub fn handle_node_update<F>(&mut self, update_fn: F)
    where
        F: Fn(NodeState) -> NodeState + Send + Sync,
    {
        self.state.node = update_fn(self.state.node.clone());
        self.state.widget_state = self.state.node.widget_state.clone_box();
    }

    pub fn handle_fee_update(&mut self, state: FeesState) {
        self.state.fees = state;
    }

    pub fn handle_key_events(&mut self, key_event: KeyEvent) -> AppResult<()> {
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