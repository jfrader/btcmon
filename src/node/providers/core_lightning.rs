// node/providers/core_lightning.rs

use anyhow::{anyhow, Result};
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

use crate::app::AppThread;
use crate::config::CoreLightningSettings;
use crate::event::Event;
use crate::node::widgets::BlockedParagraphWithGauge;
use crate::node::{NodeProvider, NodeState, NodeStatus};
use crate::widget::{DynamicNodeStatefulWidget, DynamicState};

#[derive(Debug, Deserialize)]
struct GetInfoResponse {
    pub alias: String,
    pub blockheight: u64,
    pub num_peers: u32,
    pub num_pending_channels: u32,
    pub num_active_channels: u32,
    pub num_inactive_channels: u32,
}

#[derive(Debug, Deserialize)]
struct Htlc {
    // direction: String,
    // state: String,
}

#[derive(Debug, Deserialize)]
struct Channel {
    state: String,
    #[serde(default, alias = "total_msat")]
    total_msat: u64,
    #[serde(default, alias = "to_us_msat")]
    to_us_msat: u64,
    #[serde(default)]
    htlcs: Vec<Htlc>, // HTLCs for the channel
}

#[derive(Debug, Deserialize)]
struct PeerChannelsResponse {
    channels: Vec<Channel>,
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
    pub num_peers: u32,
    pub num_pending_channels: u32,
    pub num_active_channels: u32,
    pub num_inactive_channels: u32,
    pub total_capacity: u64,
    pub local_balance: u64,
    pub num_pending_htlcs: u32, // New field for pending HTLCs
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
    fn render(&self, area: Rect, buf: &mut Buffer, node_state: &mut NodeState) {
        let mut default = CoreLightningWidgetState::default();
        let state = node_state
            .widget_state
            .as_any_mut()
            .downcast_mut::<CoreLightningWidgetState>()
            .unwrap_or(&mut default);

        let lines = vec![
            Line::from(vec![
                Span::raw("Block Height: "),
                Span::styled(node_state.height.to_string(), Style::new().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::raw("Alias: "),
                Span::styled(state.alias.clone(), Style::new().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::raw("Peers: "),
                Span::styled(state.num_peers.to_string(), Style::new().fg(Color::White)),
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
                Span::raw("Pending HTLCs: "),
                Span::styled(
                    state.num_pending_htlcs.to_string(),
                    Style::new().fg(Color::White),
                ),
            ]),
            Line::raw(""),
        ];

        let widget = BlockedParagraphWithGauge::new(
            &state.title,
            node_state.status,
            lines,
            state.local_balance,
            state.total_capacity,
        );
        widget.render(area, buf);
    }
}

#[derive(Debug)]
struct NodeInfo {
    status: NodeStatus,
    message: String,
    height: u64,
    alias: String,
    num_peers: u32,
    num_pending_channels: u32,
    num_active_channels: u32,
    num_inactive_channels: u32,
    total_capacity: u64,
    local_balance: u64,
    num_pending_htlcs: u32,
}

impl CoreLightning {
    pub fn new(settings: &CoreLightningSettings) -> Self {
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        Self {
            rest_address: settings.rest_address.clone(),
            rune: settings.rest_rune.clone(),
            client: Arc::new(client),
        }
    }

    async fn fetch_node_info(&self) -> Result<GetInfoResponse> {
        let url = format!("{}/v1/getinfo", self.rest_address);
        let response = self
            .client
            .post(&url)
            .header("Rune", &self.rune)
            .header("Content-Type", "application/json")
            .body("{}")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("CLN REST HTTP error: {}", response.status()));
        }

        Ok(response.json::<GetInfoResponse>().await?)
    }

    async fn fetch_channels(&self) -> Result<PeerChannelsResponse> {
        let url = format!("{}/v1/listpeerchannels", self.rest_address);
        let response = self
            .client
            .post(&url)
            .header("Rune", &self.rune)
            .header("Content-Type", "application/json")
            .body("{}")
            .send()
            .await?;

        let response_status = response.status();

        if !response_status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "CLN listPeers returned {}: {}",
                response_status,
                body
            ));
        }

        Ok(response.json::<PeerChannelsResponse>().await?)
    }

    async fn get_node_info(&self) -> Result<NodeInfo> {
        let info = match self.fetch_node_info().await {
            Ok(info) => info,
            Err(e) => {
                return Ok(NodeInfo {
                    status: NodeStatus::Offline,
                    message: format!("Request error: {}", e),
                    height: 0,
                    alias: String::new(),
                    num_peers: 0,
                    num_pending_channels: 0,
                    num_active_channels: 0,
                    num_inactive_channels: 0,
                    total_capacity: 0,
                    local_balance: 0,
                    num_pending_htlcs: 0,
                });
            }
        };

        let (total_capacity, local_balance, num_pending_htlcs, message) =
            match self.fetch_channels().await {
                Ok(peers) => {
                    let channels = peers.channels;

                    let capacity = channels
                        .iter()
                        .filter(|channel| channel.state == "CHANNELD_NORMAL")
                        .map(|c| c.total_msat / 1000)
                        .sum::<u64>();

                    let balance = channels
                        .iter()
                        .filter(|channel| channel.state == "CHANNELD_NORMAL")
                        .map(|c| c.to_us_msat / 1000)
                        .sum::<u64>();

                    let pending_htlcs = channels
                        .iter()
                        .flat_map(|channel| channel.htlcs.iter())
                        .count() as u32;

                    (capacity, balance, pending_htlcs, String::new())
                }
                Err(e) => (0, 0, 0, format!("Channels fetch error: {}", e)),
            };

        Ok(NodeInfo {
            status: NodeStatus::Online,
            message,
            height: info.blockheight,
            alias: info.alias,
            num_peers: info.num_peers,
            num_pending_channels: info.num_pending_channels,
            num_active_channels: info.num_active_channels,
            num_inactive_channels: info.num_inactive_channels,
            total_capacity,
            local_balance,
            num_pending_htlcs,
        })
    }

    async fn update_node_state(&self, sender: UnboundedSender<Event>, index: usize) -> Result<()> {
        let node_info = self.get_node_info().await?;

        let _ = sender.send(Event::NodeUpdate(index, Arc::new(move |mut state| {
            let widget_state = state
                .widget_state
                .as_any()
                .downcast_ref::<CoreLightningWidgetState>()
                .unwrap();

            *state
                .services
                .entry("REST".to_string())
                .or_insert(node_info.status) = node_info.status;

            if state.height > 0 && state.height < node_info.height {
                state.last_hash_instant = Some(Instant::now());
            }

            state.status = node_info.status;
            state.message = node_info.message.clone();
            state.height = node_info.height;
            state.widget_state = Box::new(CoreLightningWidgetState {
                title: widget_state.title.clone(),
                alias: node_info.alias.clone(),
                num_peers: node_info.num_peers,
                num_pending_channels: node_info.num_pending_channels,
                num_active_channels: node_info.num_active_channels,
                num_inactive_channels: node_info.num_inactive_channels,
                total_capacity: node_info.total_capacity,
                local_balance: node_info.local_balance,
                num_pending_htlcs: node_info.num_pending_htlcs,
            });

            state
        })));

        if node_info.status == NodeStatus::Offline {
            return Err(anyhow!("Node info fetch failed"));
        }
        Ok(())
    }
}

#[async_trait]
impl NodeProvider for CoreLightning {
    async fn init(&mut self, thread: AppThread, index: usize) -> Result<()> {
        let check_interval = Duration::from_secs(15);
        let host = self.rest_address.clone();

        let _ = thread
            .sender
            .send(Event::NodeUpdate(index, Arc::new(move |mut state| {
                state.host = host.clone();
                state.message = "Initializing CLN REST...".to_string();
                state
                    .services
                    .insert("REST".to_string(), NodeStatus::Offline);
                state.widget_state = Box::new(CoreLightningWidgetState {
                    title: format!("Core Lightning ({})", host),
                    ..Default::default()
                });
                state
            })));

        loop {
            if thread.token.is_cancelled() {
                break;
            }

            let _ = self.update_node_state(thread.sender.clone(), index).await;
            time::sleep(check_interval).await;
        }

        Ok(())
    }
}