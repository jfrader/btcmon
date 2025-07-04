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
    pub alias: String,
    pub blockheight: u64,
}

#[derive(Debug, Deserialize)]
struct Channel {
    state: String, // e.g., "CHANNELD_NORMAL" for active channels
    #[serde(default, alias = "msatoshi_total", alias = "total_msat")]
    msatoshi_total: u64, // Total channel capacity in millisatoshis
    #[serde(default, alias = "msatoshi_to_us", alias = "to_us_msat")]
    msatoshi_to_us: u64, // Local balance in millisatoshis
}

#[derive(Debug, Deserialize)]
struct Peer {
    channels: Vec<Channel>,
}

#[derive(Debug, Deserialize)]
struct PeersResponse {
    peers: Vec<Peer>,
}

#[derive(Clone)]
pub struct CoreLightning {
    rest_address: String,
    rune: String,
    client: Arc<Client>,
}

#[derive(Clone, Debug, Default)]
pub struct CoreLightningWidgetState {
    pub title: String,
    pub alias: String,
    pub num_channels: u64,
    pub total_capacity: u64, // Total capacity in satoshis
    pub local_balance: u64,  // Local balance in satoshis
}

impl DynamicState for CoreLightningWidgetState {
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

pub struct CoreLightningWidget;

impl DynamicNodeStatefulWidget for CoreLightningWidget {
    fn render_dynamic(
        &self,
        area: Rect,
        buf: &mut Buffer,
        node_state: &NodeState,
        state: &mut dyn DynamicState,
    ) {
        let mut default = CoreLightningWidgetState::default();
        let state = state
            .as_any_mut()
            .downcast_mut::<CoreLightningWidgetState>()
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
                    state.local_balance,
                    state.total_capacity - state.local_balance
                ),
                Style::new().bg(Color::Black),
            ))
            .ratio(if state.total_capacity > 0 {
                state.local_balance as f64 / state.total_capacity as f64
            } else {
                0.0
            });

        gauge.render(layout[1], buf);
        block.render(area, buf);
    }
}

impl CoreLightning {
    async fn get_node_info(&self, sender: UnboundedSender<Event>) -> Result<()> {
        let url = format!("{}/v1/getinfo", self.rest_address);
        let info_resp = self
            .client
            .post(&url)
            .header("Rune", &self.rune)
            .header("Content-Type", "application/json")
            .body("{}")
            .send()
            .await;

        let (status, message, height, alias, num_channels, total_capacity, local_balance) =
            match info_resp {
                Ok(resp) => {
                    let status_code = resp.status();
                    if !status_code.is_success() {
                        (
                            NodeStatus::Offline,
                            format!("CLN REST HTTP error: {}", status_code),
                            0,
                            String::new(),
                            0,
                            0,
                            0,
                        )
                    } else {
                        let info = resp.json::<GetInfoResponse>().await?;
                        let (num_channels, total_capacity, local_balance, error) =
                            match self.get_channels().await {
                                Ok(peers) => {
                                    let channels = peers
                                        .peers
                                        .iter()
                                        .flat_map(|peer| &peer.channels)
                                        .filter(|channel| channel.state == "CHANNELD_NORMAL");
                                    let num = channels.clone().count() as u64;
                                    let capacity = channels
                                        .clone()
                                        .map(|c| c.msatoshi_total / 1000)
                                        .sum::<u64>();
                                    let balance =
                                        channels.map(|c| c.msatoshi_to_us / 1000).sum::<u64>();
                                    (num, capacity, balance, String::new())
                                }
                                Err(e) => (0, 0, 0, format!("Channels fetch error: {}", e)),
                            };
                        (
                            NodeStatus::Online,
                            error,
                            info.blockheight,
                            info.alias,
                            num_channels,
                            total_capacity,
                            local_balance,
                        )
                    }
                }
                Err(e) => (
                    NodeStatus::Offline,
                    format!("Request error: {}", e),
                    0,
                    String::new(),
                    0,
                    0,
                    0,
                ),
            };

        let _ = sender.send(Event::NodeUpdate(Arc::new(move |mut state| {
            let widget_state = state
                .widget_state
                .as_any()
                .downcast_ref::<CoreLightningWidgetState>()
                .unwrap();

            *state.services.entry("REST".to_string()).or_insert(status) = status;

            if state.height > 0 && state.height < height {
                state.last_hash_instant = Some(Instant::now());
            }

            state.status = status;
            state.message = message.clone();
            state.height = height;
            state.widget_state = Box::new(CoreLightningWidgetState {
                title: widget_state.title.clone(),
                alias: alias.clone(),
                num_channels,
                total_capacity,
                local_balance,
            });

            state
        })));

        if status == NodeStatus::Offline {
            return Err(anyhow::anyhow!("Node info fetch failed"));
        }
        Ok(())
    }

    async fn get_channels(&self) -> Result<PeersResponse> {
        let url = format!("{}/v1/listpeers", self.rest_address);
        let resp = self
            .client
            .post(&url)
            .header("Rune", &self.rune)
            .header("Content-Type", "application/json")
            .body("{}")
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "CLN listPeers returned {}: {}",
                status,
                body
            ));
        }

        let peers: PeersResponse = resp.json().await?;
        Ok(peers)
    }

    async fn check_node_status(&self, sender: UnboundedSender<Event>) -> Result<()> {
        self.get_node_info(sender).await
    }
}

#[async_trait]
impl NodeProvider for CoreLightning {
    fn new(config: &AppConfig) -> Self {
        let rest_address = config.core_lightning.rest_address.clone();
        let rune = config.core_lightning.rest_rune.clone();
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        Self {
            rest_address,
            rune,
            client: Arc::new(client),
        }
    }

    async fn init(&mut self, thread: AppThread) -> Result<()> {
        let check_interval = Duration::from_secs(15);

        let _ = thread
            .sender
            .send(Event::NodeUpdate(Arc::new(move |mut state| {
                state.message = "Initializing CLN REST...".to_string();
                state
                    .services
                    .insert("REST".to_string(), NodeStatus::Offline);
                state.widget_state = Box::new(CoreLightningWidgetState {
                    title: "Core Lightning".to_string(),
                    alias: String::new(),
                    num_channels: 0,
                    total_capacity: 0,
                    local_balance: 0,
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
