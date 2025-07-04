use super::super::{AppConfig, AppThread, NodeProvider, NodeStatus};
use crate::event::Event;
use crate::node::NodeState;
use crate::ui::get_status_style;
use crate::widget::{DynamicNodeStatefulWidget, DynamicState};
use anyhow::Result;
use async_trait::async_trait;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Gauge, Padding, Paragraph, Widget};
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::{self, Duration, Instant};

#[derive(Debug, Deserialize)]
struct GetInfoResponse {
    pub block_height: u64,
    pub alias: String,
    pub num_active_channels: u64, // LND API field
}

#[derive(Debug, Deserialize)]
struct ChannelResponse {
    active: bool,
    capacity: String,      // Total capacity in satoshis
    local_balance: String, // Local balance in satoshis
    remote_balance: String,
}

#[derive(Debug, Deserialize)]
struct ChannelsResponse {
    channels: Vec<ChannelResponse>,
}

#[derive(Clone)]
pub struct LndNode {
    address: String,
    macaroon: String,
    client: Arc<Client>,
}

#[derive(Clone, Debug, Default)]
pub struct LndWidgetState {
    pub title: String,
    pub alias: String,
    pub num_channels: u64,
    pub capacity: u64,      // Total capacity in satoshis
    pub local_balance: u64, // Local balance in satoshis
    pub remote_balance: u64,
}

impl DynamicState for LndWidgetState {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn clone_box(&self) -> Box<dyn DynamicState> {
        Box::new(self.clone())
    }
}

pub struct LndWidget;

impl DynamicNodeStatefulWidget for LndWidget {
    fn render_dynamic(
        &self,
        area: Rect,
        buf: &mut Buffer,
        node_state: &NodeState,
        state: &mut dyn DynamicState,
    ) {
        let mut default = LndWidgetState::default();
        let state = state
            .as_any_mut()
            .downcast_mut::<LndWidgetState>()
            .unwrap_or(&mut default);

        let style = get_status_style(&node_state.status);
        let text = vec![
            Line::from(vec![
                Span::raw("Block Height: "),
                Span::styled(node_state.height.to_string(), Style::new().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::raw("Alias: "),
                Span::styled(state.alias.clone(), Style::new().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::raw("Open Channels: "),
                Span::styled(
                    state.num_channels.to_string(),
                    Style::new().fg(Color::White),
                ),
            ]),
            Line::raw(""), // Spacer
        ];

        let block = Block::bordered()
            .padding(Padding::left(1))
            .title(state.title.clone())
            .title_alignment(Alignment::Center)
            .border_type(BorderType::Plain)
            .style(style);

        let inner_area = block.inner(area);
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(text.len() as u16), Constraint::Length(3)])
            .split(inner_area);

        Paragraph::new(text).render(layout[0], buf);

        let gauge = Gauge::default()
            .gauge_style(Color::Green)
            .label(Span::styled(
                format!(
                    " local {} sats / remote {} sats ",
                    state.local_balance, state.remote_balance
                ),
                Style::new().bg(Color::Black),
            ))
            .ratio(if state.capacity > 0 {
                state.local_balance as f64 / state.capacity as f64
            } else {
                0.0
            });

        gauge.render(layout[1], buf);
        block.render(area, buf);
    }
}

impl LndNode {
    async fn get_node_info(&self, sender: UnboundedSender<Event>) -> Result<()> {
        let url = format!("{}/v1/getinfo", self.address);

        let response_result = self
            .client
            .get(&url)
            .header("Grpc-Metadata-macaroon", &self.macaroon)
            .send()
            .await;

        match response_result {
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() {
                    let _ = sender.send(Event::NodeUpdate(Arc::new(move |mut state| {
                        state.message = format!("LND REST error: HTTP {}", status);
                        state
                    })));
                    return Err(anyhow::anyhow!("LND REST non-200: {}", status));
                }

                let info = resp.json::<GetInfoResponse>().await?;
                let (num_channels, capacity, local_balance, remote_balance) =
                    match self.get_channels().await {
                        Ok(channels) => {
                            let active_channels = channels
                                .channels
                                .into_iter()
                                .filter(|c| c.active)
                                .collect::<Vec<_>>();
                            let num = active_channels.len() as u64;
                            let capacity = active_channels
                                .iter()
                                .map(|c| c.capacity.parse().unwrap_or(0))
                                .sum::<u64>();
                            let balance = active_channels
                                .iter()
                                .map(|c| c.local_balance.parse().unwrap_or(0))
                                .sum::<u64>();
                            let remote_balance = active_channels
                                .iter()
                                .map(|c| c.remote_balance.parse().unwrap_or(0))
                                .sum::<u64>();
                            (num, capacity, balance, remote_balance)
                        }
                        Err(_) => (info.num_active_channels, 0, 0, 0),
                    };

                let _ = sender.send(Event::NodeUpdate(Arc::new(move |mut state| {
                    let widget_state = state
                        .widget_state
                        .as_any()
                        .downcast_ref::<LndWidgetState>()
                        .unwrap();

                    if state.height > 0 && state.height < info.block_height {
                        state.last_hash_instant = Some(Instant::now());
                    }

                    state.message = "".to_string();
                    state.status = NodeStatus::Online;
                    state.height = info.block_height;
                    *state
                        .services
                        .entry("REST".to_string())
                        .or_insert(NodeStatus::Online) = NodeStatus::Online;
                    state.widget_state = Box::new(LndWidgetState {
                        title: widget_state.title.clone(),
                        alias: info.alias.clone(),
                        num_channels,
                        capacity,
                        local_balance,
                        remote_balance,
                    });
                    state
                })));

                Ok(())
            }
            Err(e) => Err(anyhow::anyhow!("Request error: {}", e)),
        }
    }

    async fn get_channels(&self) -> Result<ChannelsResponse> {
        let url = format!("{}/v1/channels", self.address);
        let resp = self
            .client
            .get(&url)
            .header("Grpc-Metadata-macaroon", &self.macaroon)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "LND channels returned {}: {}",
                status,
                body
            ));
        }

        let channels: ChannelsResponse = resp.json().await?;
        Ok(channels)
    }

    async fn check_node_status(&self, sender: UnboundedSender<Event>) -> Result<()> {
        match self.get_node_info(sender.clone()).await {
            Ok(_) => Ok(()),
            Err(e) => {
                let _ = sender.send(Event::NodeUpdate(Arc::new(|mut state| {
                    state.status = NodeStatus::Offline;
                    *state
                        .services
                        .entry("REST".to_string())
                        .or_insert(NodeStatus::Offline) = NodeStatus::Offline;

                    state
                })));
                Err(e)
            }
        }
    }
}

#[async_trait]
impl NodeProvider for LndNode {
    fn new(config: &AppConfig) -> Self {
        let address = config.lnd.rest_address.clone();
        let macaroon = config.lnd.macaroon_hex.clone();
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        Self {
            address,
            macaroon,
            client: Arc::new(client),
        }
    }

    async fn init(&mut self, thread: AppThread) -> Result<()> {
        let check_interval = Duration::from_secs(15);

        let host = self.address.clone();

        let _ = thread
            .sender
            .send(Event::NodeUpdate(Arc::new(move |mut state| {
                state.host = host.clone();
                state.message = "Initializing LND REST...".to_string();
                state
                    .services
                    .insert("REST".to_string(), NodeStatus::Offline);
                state.widget_state = Box::new(LndWidgetState {
                    title: "LND".to_string(),
                    alias: "".to_string(),
                    num_channels: 0,
                    capacity: 0,
                    local_balance: 0,
                    remote_balance: 0,
                });
                state
            })));

        loop {
            if thread.token.is_cancelled() {
                break;
            }

            let _ = self.check_node_status(thread.sender.clone()).await;

            time::sleep(check_interval).await;
        }

        Ok(())
    }
}
