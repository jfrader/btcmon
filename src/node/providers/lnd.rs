use anyhow::Result;
use async_trait::async_trait;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::{self, Duration, Instant};

use crate::config::{AppConfig, LndSettings};
use crate::event::Event;
use crate::node::widgets::{BlockedParagraph, BlockedParagraphWithGauge};
use crate::node::{NodeState, NodeStatus};
use crate::widget::{DynamicNodeStatefulWidget, DynamicState};
use crate::{app::AppThread, node::NodeProvider};

#[derive(Debug, Deserialize)]
struct GetInfoResponse {
    pub block_height: u64,
    pub alias: String,
    pub num_active_channels: u64,
    pub num_pending_channels: u64,
    pub num_inactive_channels: u64,
    pub num_peers: u32,
    pub synced_to_chain: bool,
    pub synced_to_graph: bool,
}

#[derive(Debug, Deserialize)]
struct Htlc {
    // incoming: bool,
    // Add other relevant fields based on LND API
}

#[derive(Debug, Deserialize)]
struct ChannelResponse {
    active: bool,
    capacity: String,
    local_balance: String,
    remote_balance: String,
    #[serde(default)]
    pending_htlcs: Option<Vec<Htlc>>,
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
    pub num_peers: u32,
    pub num_pending_channels: u64,
    pub num_active_channels: u64,
    pub num_inactive_channels: u64,
    pub capacity: u64,
    pub local_balance: u64,
    pub remote_balance: u64,
    pub synced_to_chain: bool,
    pub synced_to_graph: bool,
    pub num_pending_htlcs: u64,
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
    fn render(&self, area: Rect, buf: &mut Buffer, node_state: &mut NodeState, config: &AppConfig) {
        let mut default = LndWidgetState::default();
        let state = node_state
            .widget_state
            .as_any_mut()
            .downcast_mut::<LndWidgetState>()
            .unwrap_or(&mut default);

        let block_height = match node_state.status {
            NodeStatus::Synchronizing => Line::from(vec![
                Span::raw("Block Height: "),
                Span::styled(node_state.height.to_string(), Style::new().fg(Color::White)),
            ]),
            _ => Line::from(vec![
                Span::raw("Block Height: "),
                Span::styled(node_state.height.to_string(), Style::new().fg(Color::White)),
            ]),
        };

        let alias_text = match config.streamer_mode {
            true => "****".to_string(),
            false => state.alias.clone(),
        };

        let lines = vec![
            block_height,
            Line::from(vec![
                Span::raw("Alias: "),
                Span::styled(alias_text, Style::new().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::raw("Active Channels: "),
                Span::styled(
                    state.num_active_channels.to_string(),
                    Style::new().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::raw("Pending Channels: "),
                Span::styled(
                    state.num_pending_channels.to_string(),
                    Style::new().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::raw("Inactive Channels: "),
                Span::styled(
                    state.num_inactive_channels.to_string(),
                    Style::new().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::raw("Synced to Bitcoin: "),
                Span::styled(
                    if state.synced_to_chain {
                        "True"
                    } else {
                        "False"
                    },
                    Style::new().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::raw("Synced to Lightning: "),
                Span::styled(
                    if state.synced_to_graph {
                        "True"
                    } else {
                        "False"
                    },
                    Style::new().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::raw("Peers: "),
                Span::styled(state.num_peers.to_string(), Style::new().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::raw("Pending HTLCs: "),
                Span::styled(
                    state.num_pending_htlcs.to_string(),
                    Style::new().fg(Color::White),
                ),
            ]),
            Line::raw(""),
        ];

        if config.streamer_mode {
            let widget = BlockedParagraph::new(&state.title, node_state.status, lines);
            widget.render(area, buf);
        } else {
            let widget = BlockedParagraphWithGauge::new(
                &state.title,
                node_state.status,
                lines,
                state.local_balance,
                state.capacity,
            );
            widget.render(area, buf);
        }
    }
}

impl LndNode {
    pub fn new(settings: &LndSettings) -> Self {
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        Self {
            address: settings.rest_address.clone(),
            macaroon: settings.macaroon_hex.clone(),
            client: Arc::new(client),
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

    async fn get_node_info(&self, sender: UnboundedSender<Event>, index: usize) -> Result<()> {
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
                    let _ = sender.send(Event::NodeUpdate(
                        index,
                        Arc::new(move |mut state| {
                            state.message = format!("LND REST error: HTTP {}", status);
                            state
                        }),
                    ));
                    return Err(anyhow::anyhow!("LND REST non-200: {}", status));
                }

                let info = resp.json::<GetInfoResponse>().await?;
                let empty_htlcs = vec![];
                let (capacity, local_balance, remote_balance, num_pending_htlcs) =
                    match self.get_channels().await {
                        Ok(channels) => {
                            let active_channels = channels
                                .channels
                                .iter()
                                .filter(|c| c.active)
                                .collect::<Vec<_>>();
                            let capacity = active_channels
                                .iter()
                                .map(|c| c.capacity.parse().unwrap_or(0))
                                .sum::<u64>();
                            let local_balance = active_channels
                                .iter()
                                .map(|c| c.local_balance.parse().unwrap_or(0))
                                .sum::<u64>();
                            let remote_balance = active_channels
                                .iter()
                                .map(|c| c.remote_balance.parse().unwrap_or(0))
                                .sum::<u64>();
                            let pending_htlcs = channels
                                .channels
                                .iter()
                                .flat_map(|c| c.pending_htlcs.as_ref().unwrap_or(&empty_htlcs))
                                .count() as u64;
                            (capacity, local_balance, remote_balance, pending_htlcs)
                        }
                        Err(_) => (0, 0, 0, 0),
                    };

                let new_status = if info.synced_to_chain && info.synced_to_graph {
                    NodeStatus::Online
                } else {
                    NodeStatus::Synchronizing
                };

                let _ = sender.send(Event::NodeUpdate(
                    index,
                    Arc::new(move |mut state| {
                        let widget_state = state
                            .widget_state
                            .as_any()
                            .downcast_ref::<LndWidgetState>()
                            .unwrap();

                        if state.height > 0 && state.height < info.block_height {
                            state.last_hash_instant = Some(Instant::now());
                        }

                        state.message = "".to_string();
                        state.status = new_status;
                        state.height = info.block_height;
                        *state
                            .services
                            .entry("REST".to_string())
                            .or_insert(NodeStatus::Online) = NodeStatus::Online;
                        state.widget_state = Box::new(LndWidgetState {
                            title: widget_state.title.clone(),
                            alias: info.alias.clone(),
                            num_peers: info.num_peers,
                            num_pending_channels: info.num_pending_channels,
                            num_active_channels: info.num_active_channels,
                            num_inactive_channels: info.num_inactive_channels,
                            capacity,
                            local_balance,
                            remote_balance,
                            synced_to_chain: info.synced_to_chain,
                            synced_to_graph: info.synced_to_graph,
                            num_pending_htlcs,
                        });
                        state
                    }),
                ));

                Ok(())
            }
            Err(e) => {
                let _ = sender.send(Event::NodeUpdate(
                    index,
                    Arc::new(|mut state| {
                        state.status = NodeStatus::Offline;
                        *state
                            .services
                            .entry("REST".to_string())
                            .or_insert(NodeStatus::Offline) = NodeStatus::Offline;
                        state
                    }),
                ));
                Err(anyhow::anyhow!("Request error: {}", e))
            }
        }
    }
    async fn check_node_status(&self, sender: UnboundedSender<Event>, index: usize) -> Result<()> {
        self.get_node_info(sender, index).await
    }
}

#[async_trait]
impl NodeProvider for LndNode {
    async fn init(&mut self, thread: AppThread, index: usize) -> Result<()> {
        let check_interval = Duration::from_secs(15);

        let host = self.address.clone();

        let _ = thread.sender.send(Event::NodeUpdate(
            index,
            Arc::new(move |mut state| {
                state.host = host.clone();
                state.message = "Initializing LND REST...".to_string();
                state
                    .services
                    .insert("REST".to_string(), NodeStatus::Offline);
                state.widget_state = Box::new(LndWidgetState {
                    title: format!("LND ({})", host),
                    alias: "".to_string(),
                    num_peers: 0,
                    num_pending_channels: 0,
                    num_active_channels: 0,
                    num_inactive_channels: 0,
                    capacity: 0,
                    local_balance: 0,
                    remote_balance: 0,
                    synced_to_chain: false,
                    synced_to_graph: false,
                    num_pending_htlcs: 0,
                });
                state
            }),
        ));

        loop {
            if thread.token.is_cancelled() {
                break;
            }

            let _ = self.check_node_status(thread.sender.clone(), index).await;

            time::sleep(check_interval).await;
        }

        Ok(())
    }
}
