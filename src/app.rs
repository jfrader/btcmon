use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::collections::VecDeque;
use std::error;
use std::str::FromStr;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardView {
    Overview,
    Node,
    Price,
    Fees,
}

impl DashboardView {
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "OVERVIEW",
            Self::Node => "NODE",
            Self::Price => "PRICE",
            Self::Fees => "FEES",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchAction {
    PreviousNode,
    NextNode,
    ToggleNodeRotation,
    ToggleViewMenu,
    SelectView(DashboardView),
    DismissViewMenu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TouchTarget {
    pub area: Rect,
    pub action: TouchAction,
}

impl TouchTarget {
    fn contains(self, x: u16, y: u16) -> bool {
        x >= self.area.x
            && x < self.area.x.saturating_add(self.area.width)
            && y >= self.area.y
            && y < self.area.y.saturating_add(self.area.height)
    }
}

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
    pub price_history: VecDeque<(Instant, f64)>,
}

pub struct App {
    pub nodes: Vec<Node>,
    pub node_names: Vec<String>,
    pub current_node_index: usize,
    pub last_node_switch: Option<Instant>,
    pub node_switch_interval: Duration,
    pub seconds_until_rotation: u64,
    pub thread: AppThread,
    pub config: AppConfig,
    pub widgets: Vec<Box<dyn DynamicNodeStatefulWidget>>,
    pub state: AppState,
    pub running: bool,
    pub active_view: DashboardView,
    pub view_menu_open: bool,
    pub auto_rotate_nodes: bool,
    pub touch_targets: Vec<TouchTarget>,
}

impl App {
    pub fn new(
        thread: AppThread,
        widgets: Vec<Box<dyn DynamicNodeStatefulWidget>>,
        widget_states: Vec<Box<dyn DynamicState>>,
        node_names: Vec<String>,
        config: AppConfig,
    ) -> Self {
        let cloned_thread = thread.clone();
        let interval = Duration::from_secs(
            config
                .node_switch_interval
                .parse::<u64>()
                .unwrap_or(5)
                .max(1),
        );
        let num_nodes = widgets.len();
        let node_names = (0..num_nodes)
            .map(|index| {
                node_names
                    .get(index)
                    .filter(|name| !name.trim().is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("Node {}", index + 1))
            })
            .collect();
        let active_view = if num_nodes == 0 {
            DashboardView::Price
        } else if config.price.enabled || config.fees.enabled {
            DashboardView::Overview
        } else {
            DashboardView::Node
        };

        Self {
            running: true,
            config,
            thread,
            nodes: (0..num_nodes)
                .map(|_| Node::new(cloned_thread.clone()))
                .collect(),
            node_names,
            current_node_index: 0,
            last_node_switch: None,
            node_switch_interval: interval,
            seconds_until_rotation: interval.as_secs(),
            widgets,
            state: AppState {
                counter: 0,
                price: PriceState::new(),
                fees: FeesState::new(),
                price_history: VecDeque::new(),
                node_states: widget_states
                    .into_iter()
                    .map(|ws| {
                        let mut ns = NodeState::new();
                        ns.widget_state = ws;
                        ns.current_node_index = 0; // Will be updated in tick
                        ns.total_nodes = num_nodes;
                        ns.seconds_until_rotation = interval.as_secs();
                        ns
                    })
                    .collect(),
            },
            active_view,
            view_menu_open: false,
            auto_rotate_nodes: num_nodes > 1,
            touch_targets: Vec::new(),
        }
    }

    pub fn available_views(&self) -> Vec<DashboardView> {
        let has_nodes = !self.nodes.is_empty() && !self.state.node_states.is_empty();
        let mut views = Vec::with_capacity(4);

        if has_nodes && (self.config.price.enabled || self.config.fees.enabled) {
            views.push(DashboardView::Overview);
        }
        if has_nodes {
            views.push(DashboardView::Node);
        }
        if self.config.price.enabled || !has_nodes {
            views.push(DashboardView::Price);
        }
        if self.config.fees.enabled {
            views.push(DashboardView::Fees);
        }

        views
    }

    pub fn select_view(&mut self, view: DashboardView) {
        if self.available_views().contains(&view) {
            self.active_view = view;
        }
        self.view_menu_open = false;
    }

    pub fn cycle_view(&mut self, forwards: bool) {
        let views = self.available_views();
        if views.len() < 2 {
            return;
        }

        let current = views
            .iter()
            .position(|view| *view == self.active_view)
            .unwrap_or(0);
        let next = if forwards {
            (current + 1) % views.len()
        } else if current == 0 {
            views.len() - 1
        } else {
            current - 1
        };
        self.select_view(views[next]);
    }

    pub fn next_node(&mut self) {
        if self.nodes.len() > 1 {
            self.current_node_index = (self.current_node_index + 1) % self.nodes.len();
            self.auto_rotate_nodes = false;
            self.reset_node_rotation();
        }
    }

    pub fn previous_node(&mut self) {
        if self.nodes.len() > 1 {
            self.current_node_index = if self.current_node_index == 0 {
                self.nodes.len() - 1
            } else {
                self.current_node_index - 1
            };
            self.auto_rotate_nodes = false;
            self.reset_node_rotation();
        }
    }

    pub fn toggle_node_rotation(&mut self) {
        if self.nodes.len() > 1 {
            self.auto_rotate_nodes = !self.auto_rotate_nodes;
            self.reset_node_rotation();
        }
    }

    pub fn current_node_name(&self) -> String {
        self.node_names
            .get(self.current_node_index)
            .cloned()
            .unwrap_or_else(|| format!("Node {}", self.current_node_index + 1))
    }

    fn reset_node_rotation(&mut self) {
        self.last_node_switch = Some(Instant::now());
        self.seconds_until_rotation = self.node_switch_interval.as_secs();
    }

    pub fn init_price(&mut self) {
        let currency =
            PriceCurrency::from_str(&self.config.price.currency).unwrap_or(PriceCurrency::USD);
        spawn_price_checker::<PriceCoinbase>(self.thread.clone(), currency);
    }

    pub fn init_fees(&mut self) {
        spawn_fees_checker::<FeesBlockchainInfo>(self.thread.clone());
    }

    pub fn tick(&mut self) {
        if self.nodes.len() > 1 && self.auto_rotate_nodes {
            let now = Instant::now();
            if let Some(last_switch) = self.last_node_switch {
                let elapsed = now.duration_since(last_switch).as_secs();
                self.seconds_until_rotation =
                    self.node_switch_interval.as_secs().saturating_sub(elapsed);
                if elapsed >= self.node_switch_interval.as_secs() {
                    self.current_node_index = (self.current_node_index + 1) % self.nodes.len();
                    self.last_node_switch = Some(now);
                    self.seconds_until_rotation = self.node_switch_interval.as_secs();
                }
            } else {
                self.last_node_switch = Some(now);
            }
        } else if self.nodes.len() > 1 {
            self.seconds_until_rotation = self.node_switch_interval.as_secs();
        }

        for node_state in &mut self.state.node_states {
            node_state.tick();
            node_state.current_node_index = self.current_node_index;
            node_state.total_nodes = self.nodes.len();
            node_state.seconds_until_rotation = self.seconds_until_rotation;
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

    pub fn handle_price_update(&mut self, mut state: PriceState) {
        if state.last_price_in_currency.is_none() {
            state.last_price_in_currency = self.state.price.last_price_in_currency;
        }
        if state.last_ok_at.is_none() {
            state.last_ok_at = self.state.price.last_ok_at;
        }
        let record_history = state.last_error.is_none();
        self.state.price = state;
        if !record_history {
            return;
        }
        let now = Instant::now();
        if let Some(price) = self.state.price.last_price_in_currency {
            self.state.price_history.push_back((now, price));
            let max_age = Duration::from_secs(60 * 60 * 24);
            while let Some((timestamp, _)) = self.state.price_history.front() {
                if now.duration_since(*timestamp) > max_age {
                    self.state.price_history.pop_front();
                } else {
                    break;
                }
            }
        }
    }

    pub fn handle_node_update(
        &mut self,
        index: usize,
        update_fn: &(dyn Fn(NodeState) -> NodeState + Send + Sync),
    ) {
        let updated = update_fn(self.state.node_states[index].clone());
        self.state.node_states[index] = updated;
    }

    pub fn handle_fee_update(&mut self, mut state: FeesState) {
        if state.result.is_empty() {
            state.result = self.state.fees.result.clone();
        }
        self.state.fees = state;
    }

    pub fn handle_key_events(&mut self, key_event: KeyEvent) -> AppResult<()> {
        match key_event.code {
            KeyCode::Esc => {
                if self.view_menu_open {
                    self.view_menu_open = false;
                } else {
                    self.quit();
                }
            }
            KeyCode::Char('q') => {
                self.quit();
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                if key_event.modifiers == KeyModifiers::CONTROL {
                    self.quit();
                }
            }
            KeyCode::Right | KeyCode::Char('n') => {
                self.next_node();
            }
            KeyCode::Left => {
                self.previous_node();
            }
            KeyCode::Up => {
                if self.nodes.len() > 1 {
                    let new_interval = self.node_switch_interval.as_secs().saturating_add(1);
                    self.node_switch_interval = Duration::from_secs(new_interval);
                    self.seconds_until_rotation = new_interval;
                    self.last_node_switch = Some(Instant::now());
                }
            }
            KeyCode::Down => {
                if self.nodes.len() > 1 {
                    let new_interval = self.node_switch_interval.as_secs().saturating_sub(1);
                    self.node_switch_interval = Duration::from_secs(new_interval.max(1));
                    self.seconds_until_rotation = new_interval.max(1);
                    self.last_node_switch = Some(Instant::now());
                }
            }
            KeyCode::Tab => self.cycle_view(true),
            KeyCode::BackTab => self.cycle_view(false),
            KeyCode::Char('v') => {
                if self.available_views().len() > 1 {
                    self.view_menu_open = !self.view_menu_open;
                }
            }
            KeyCode::Char('r') | KeyCode::Char(' ') => self.toggle_node_rotation(),
            KeyCode::Char('1') => {
                if let Some(view) = self.available_views().first().copied() {
                    self.select_view(view);
                }
            }
            KeyCode::Char('2') => {
                if let Some(view) = self.available_views().get(1).copied() {
                    self.select_view(view);
                }
            }
            KeyCode::Char('3') => {
                if let Some(view) = self.available_views().get(2).copied() {
                    self.select_view(view);
                }
            }
            KeyCode::Char('4') => {
                if let Some(view) = self.available_views().get(3).copied() {
                    self.select_view(view);
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn handle_mouse_events(&mut self, mouse_event: MouseEvent) -> AppResult<()> {
        if let MouseEventKind::Down(_) = mouse_event.kind {
            let action = self
                .touch_targets
                .iter()
                .rev()
                .find(|target| target.contains(mouse_event.column, mouse_event.row))
                .map(|target| target.action);

            if let Some(action) = action {
                self.handle_touch_action(action);
            }
        }
        Ok(())
    }

    fn handle_touch_action(&mut self, action: TouchAction) {
        match action {
            TouchAction::PreviousNode => self.previous_node(),
            TouchAction::NextNode => self.next_node(),
            TouchAction::ToggleNodeRotation => self.toggle_node_rotation(),
            TouchAction::ToggleViewMenu => {
                if self.available_views().len() > 1 {
                    self.view_menu_open = !self.view_menu_open;
                }
            }
            TouchAction::SelectView(view) => self.select_view(view),
            TouchAction::DismissViewMenu => self.view_menu_open = false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        BitcoinCoreSettings, CoreLightningSettings, FeesSettings, LndSettings, NodeConfig,
        PriceSettings,
    };
    use crate::widget::DefaultWidgetState;
    use crossterm::event::{KeyModifiers, MouseButton};
    use ratatui::buffer::Buffer;

    struct TestNodeWidget;

    impl DynamicNodeStatefulWidget for TestNodeWidget {
        fn render(
            &self,
            _area: Rect,
            _buf: &mut Buffer,
            _node_state: &mut NodeState,
            _config: &AppConfig,
        ) {
        }
    }

    fn test_config(node_count: usize, price: bool, fees: bool) -> AppConfig {
        AppConfig {
            tick_rate: "250".to_string(),
            streamer_mode: false,
            node_switch_interval: "5".to_string(),
            price: PriceSettings {
                enabled: price,
                currency: "USD".to_string(),
                big_text: true,
                variation: "minute".to_string(),
                variation_threshold: 0.0,
            },
            fees: FeesSettings { enabled: fees },
            bitcoin_core: BitcoinCoreSettings::default(),
            core_lightning: CoreLightningSettings::default(),
            lnd: LndSettings::default(),
            nodes: (0..node_count)
                .map(|index| NodeConfig {
                    name: Some(format!("Node {}", index + 1)),
                    provider: "bitcoin_core".to_string(),
                    bitcoin_core: None,
                    core_lightning: None,
                    lnd: None,
                })
                .collect(),
        }
    }

    fn test_app(node_count: usize, price: bool, fees: bool) -> App {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let widgets = (0..node_count)
            .map(|_| Box::new(TestNodeWidget) as Box<dyn DynamicNodeStatefulWidget>)
            .collect();
        let states = (0..node_count)
            .map(|_| Box::new(DefaultWidgetState) as Box<dyn DynamicState>)
            .collect();
        let node_names = (0..node_count)
            .map(|index| format!("Node {}", index + 1))
            .collect();
        App::new(
            AppThread::new(sender),
            widgets,
            states,
            node_names,
            test_config(node_count, price, fees),
        )
    }

    #[test]
    fn exposes_only_views_backed_by_enabled_sources() {
        let mut app = test_app(2, true, true);

        assert_eq!(
            app.available_views(),
            vec![
                DashboardView::Overview,
                DashboardView::Node,
                DashboardView::Price,
                DashboardView::Fees,
            ]
        );
        assert_eq!(app.active_view, DashboardView::Overview);

        app.cycle_view(true);
        assert_eq!(app.active_view, DashboardView::Node);
        app.cycle_view(false);
        assert_eq!(app.active_view, DashboardView::Overview);

        let price_only = test_app(0, true, false);
        assert_eq!(price_only.available_views(), vec![DashboardView::Price]);
        assert_eq!(price_only.active_view, DashboardView::Price);
    }

    #[test]
    fn manual_node_navigation_wraps_and_rotation_can_be_pinned() {
        let mut app = test_app(2, true, true);

        app.previous_node();
        assert_eq!(app.current_node_index, 1);
        assert!(!app.auto_rotate_nodes);
        app.next_node();
        assert_eq!(app.current_node_index, 0);
        assert!(!app.auto_rotate_nodes);
        assert!(app.last_node_switch.is_some());

        app.toggle_node_rotation();
        assert!(app.auto_rotate_nodes);
        app.toggle_node_rotation();
        assert!(!app.auto_rotate_nodes);
        app.last_node_switch = Some(Instant::now() - Duration::from_secs(20));
        app.tick();
        assert_eq!(app.current_node_index, 0);
    }

    #[test]
    fn automatic_rotation_updates_the_selected_node_and_render_state_together() {
        let mut app = test_app(2, true, false);
        app.last_node_switch = Some(Instant::now() - Duration::from_secs(6));

        app.tick();

        assert_eq!(app.current_node_index, 1);
        assert!(app
            .state
            .node_states
            .iter()
            .all(|state| state.current_node_index == 1));
    }

    #[test]
    fn rotation_interval_is_never_less_than_one_second() {
        let mut config = test_config(2, true, false);
        config.node_switch_interval = "0".to_string();
        let (sender, _receiver) = mpsc::unbounded_channel();
        let widgets = (0..2)
            .map(|_| Box::new(TestNodeWidget) as Box<dyn DynamicNodeStatefulWidget>)
            .collect();
        let states = (0..2)
            .map(|_| Box::new(DefaultWidgetState) as Box<dyn DynamicState>)
            .collect();

        let app = App::new(
            AppThread::new(sender),
            widgets,
            states,
            vec!["One".to_string(), "Two".to_string()],
            config,
        );

        assert_eq!(app.node_switch_interval, Duration::from_secs(1));
    }

    #[test]
    fn failed_price_update_keeps_the_last_good_value() {
        let mut app = test_app(0, true, false);
        app.handle_price_update(PriceState {
            currency: PriceCurrency::USD,
            last_price_in_currency: Some(100_000.0),
            last_error: None,
            last_ok_at: None,
        });
        app.handle_price_update(PriceState {
            currency: PriceCurrency::USD,
            last_price_in_currency: None,
            last_error: Some("connection reset".to_string()),
            last_ok_at: None,
        });

        assert_eq!(app.state.price.last_price_in_currency, Some(100_000.0));
        assert_eq!(
            app.state.price.last_error.as_deref(),
            Some("connection reset")
        );
        assert_eq!(app.state.price_history.len(), 1);
    }

    #[test]
    fn failed_fee_update_keeps_the_last_good_value() {
        let mut app = test_app(0, true, true);
        app.handle_fee_update(FeesState {
            result: crate::fees::FeeResult {
                low: None,
                medium: Some("12".to_string()),
                high: Some("20".to_string()),
            },
            last_error: None,
        });
        app.handle_fee_update(FeesState {
            result: crate::fees::FeeResult {
                low: None,
                medium: None,
                high: None,
            },
            last_error: Some("timed out".to_string()),
        });

        assert_eq!(app.state.fees.result.medium.as_deref(), Some("12"));
        assert_eq!(app.state.fees.last_error.as_deref(), Some("timed out"));
    }

    #[test]
    fn touch_actions_use_the_rectangles_provided_by_the_renderer() {
        let mut app = test_app(2, true, false);
        app.touch_targets = vec![TouchTarget {
            area: Rect::new(4, 7, 10, 3),
            action: TouchAction::NextNode,
        }];

        app.handle_mouse_events(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 8,
            row: 8,
            modifiers: KeyModifiers::NONE,
        })
        .unwrap();
        assert_eq!(app.current_node_index, 1);

        app.handle_mouse_events(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 8,
            modifiers: KeyModifiers::NONE,
        })
        .unwrap();
        assert_eq!(app.current_node_index, 1);
    }
}
