use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::str::FromStr;
use std::{error};
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::config::AppConfig;
use crate::event::Event;
use crate::fees::providers::FeesBlockchainInfo;
use crate::fees::{spawn_fees_checker, FeesState};
use crate::node::{Node, NodeState};
use crate::price::providers::coinbase::PriceCoinbase;
use crate::price::{spawn_price_checker, PriceCurrency, PriceState};
use crate::widget::{DynamicNodeStatefulWidget, DynamicState};

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
    pub node_states: Vec<NodeState>,
}

pub struct App {
    pub nodes: Vec<Node>,
    pub current_node_index: usize,
    pub last_node_switch: Option<Instant>,
    pub node_switch_interval: Duration,
    pub thread: AppThread,
    pub config: AppConfig,
    pub widgets: Vec<Box<dyn DynamicNodeStatefulWidget>>,
    pub state: AppState,
    pub running: bool,
}

impl App {
    pub fn new(
        thread: AppThread,
        widgets: Vec<Box<dyn DynamicNodeStatefulWidget>>,
        widget_states: Vec<Box<dyn DynamicState>>,
        config: AppConfig,
    ) -> Self {
        let cloned_thread = thread.clone();
        let num_nodes = widgets.len();
        Self {
            running: true,
            config,
            thread,
            nodes: (0..num_nodes).map(|_| Node::new(cloned_thread.clone())).collect(),
            current_node_index: 0,
            last_node_switch: None,
            node_switch_interval: Duration::from_secs(5),
            widgets,
            state: AppState {
                counter: 0,
                price: PriceState::new(),
                fees: FeesState::new(),
                node_states: widget_states.into_iter().map(|ws| {
                    let mut ns = NodeState::new();
                    ns.widget_state = ws;
                    ns
                }).collect(),
            },
        }
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
        for node_state in &mut self.state.node_states {
            node_state.tick();
        }

        if self.nodes.len() > 1 {
            let now = Instant::now();
            let should_advance = match self.last_node_switch {
                Some(last) => now.duration_since(last) >= self.node_switch_interval,
                None => true,
            };

            if should_advance {
                self.current_node_index = (self.current_node_index + 1) % self.nodes.len();
                self.last_node_switch = Some(now);
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

    pub fn handle_node_update(&mut self, index: usize, update_fn: &(dyn Fn(NodeState) -> NodeState + Send + Sync)) {
        let updated = update_fn(self.state.node_states[index].clone());
        self.state.node_states[index] = updated;
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
            KeyCode::Right | KeyCode::Char('n') => {
                if self.nodes.len() > 1 {
                    self.current_node_index = (self.current_node_index + 1) % self.nodes.len();
                    self.last_node_switch = Some(Instant::now());
                }
            }
            KeyCode::Left => {
                if self.nodes.len() > 1 {
                    self.current_node_index = if self.current_node_index == 0 {
                        self.nodes.len() - 1
                    } else {
                        self.current_node_index - 1
                    };
                    self.last_node_switch = Some(Instant::now());
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn handle_mouse_events(&mut self, mouse_event: MouseEvent) -> AppResult<()> {
        if self.nodes.len() > 1 {
            match mouse_event.kind {
                MouseEventKind::Down(_) => {
                    // Get the mouse coordinates
                    // let x = mouse_event.column;
                    let y = mouse_event.row;

                    // Assuming the top panel is the node UI (first section in ui/mod.rs layout)
                    // This is a rough check; adjust based on actual layout constraints
                    if y < (self.config.tick_rate.parse::<u64>().unwrap() as u16 / 2) { // Half the screen height as a proxy
                        self.current_node_index = (self.current_node_index + 1) % self.nodes.len();
                        self.last_node_switch = Some(Instant::now());
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}
