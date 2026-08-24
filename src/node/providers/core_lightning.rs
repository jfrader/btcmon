// node/providers/core_lightning.rs

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::{self, Duration, Instant};

use crate::app::AppThread;
use crate::config::{AppConfig, CoreLightningSettings};
use crate::event::Event;
use crate::node::widgets::{BlockedParagraph, BlockedParagraphWithGauge};
use crate::node::{NodeProvider, NodeState, NodeStatus};
use crate::widget::{DynamicNodeStatefulWidget, DynamicState};

#[derive(Debug, Deserialize)]
struct GetInfoResponse {
    pub alias: String,
    pub blockheight: u64,
    #[serde(default)]
    pub num_peers: u32,
    #[serde(default)]
    pub num_pending_channels: u32,
    #[serde(default)]
    pub num_active_channels: u32,
    #[serde(default)]
    pub num_inactive_channels: u32,
}

#[derive(Debug, Deserialize, Default)]
struct Htlc {}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Msat {
    Num(u64),
    Text(String),
}

impl Default for Msat {
    fn default() -> Self {
        Self::Num(0)
    }
}

impl Msat {
    fn msats(&self) -> u64 {
        match self {
            Self::Num(value) => *value,
            Self::Text(text) => parse_msat(text),
        }
    }
}

fn parse_msat(text: &str) -> u64 {
    let trimmed = text.trim();
    let number = trimmed
        .strip_suffix("msat")
        .or_else(|| trimmed.strip_suffix("sat"))
        .unwrap_or(trimmed)
        .trim();
    let parsed = number.parse::<u64>().unwrap_or(0);
    if trimmed.ends_with("msat") || !trimmed.ends_with("sat") {
        parsed
    } else {
        parsed.saturating_mul(1000)
    }
}

#[derive(Debug, Deserialize)]
struct Channel {
    state: String,
    #[serde(default)]
    peer_connected: Option<bool>,
    #[serde(default, alias = "total_msat")]
    total_msat: Msat,
    #[serde(default, alias = "to_us_msat")]
    to_us_msat: Msat,
    #[serde(default)]
    htlcs: Vec<Htlc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelBucket {
    Up,
    Pending,
    Down,
}

fn classify_channel(channel: &Channel) -> ChannelBucket {
    match channel.state.as_str() {
        "CHANNELD_NORMAL" => {
            if channel.peer_connected.unwrap_or(true) {
                ChannelBucket::Up
            } else {
                ChannelBucket::Down
            }
        }
        "OPENINGD"
        | "CHANNELD_AWAITING_LOCKIN"
        | "DUALOPEND_OPEN_INIT"
        | "DUALOPEND_AWAITING_LOCKIN"
        | "DUALOPEND_OPEN_COMMITTED"
        | "DUALOPEND_OPEN_COMMIT_READY" => ChannelBucket::Pending,
        _ => ChannelBucket::Down,
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ChannelSummary {
    up: u32,
    pending: u32,
    down: u32,
    capacity_sat: u64,
    local_sat: u64,
    pending_htlcs: u32,
}

fn summarize_channels(channels: &[Channel]) -> ChannelSummary {
    let mut summary = ChannelSummary::default();
    for channel in channels {
        match classify_channel(channel) {
            ChannelBucket::Up => summary.up += 1,
            ChannelBucket::Pending => summary.pending += 1,
            ChannelBucket::Down => summary.down += 1,
        }
        if channel.state == "CHANNELD_NORMAL" {
            summary.capacity_sat += channel.total_msat.msats() / 1000;
            summary.local_sat += channel.to_us_msat.msats() / 1000;
        }
        summary.pending_htlcs += channel.htlcs.len() as u32;
    }
    summary
}

#[derive(Debug, Deserialize)]
struct PeerChannelsResponse {
    channels: Vec<Channel>,
}

fn parse_watchtower_count(value: &Value) -> Option<u32> {
    value
        .get("towers")
        .and_then(|towers| towers.as_array())
        .map(|towers| towers.len() as u32)
        .or_else(|| {
            value
                .get("watchtowers")
                .and_then(|towers| towers.as_array())
                .map(|towers| towers.len() as u32)
        })
        .or_else(|| {
            value
                .get("towers")
                .and_then(|towers| towers.as_object())
                .map(|towers| towers.len() as u32)
        })
        .or_else(|| value.as_object().map(|towers| towers.len() as u32))
        .or_else(|| {
            value
                .get("num_towers")
                .and_then(|count| count.as_u64())
                .map(|count| count as u32)
        })
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
    pub num_watchtowers: Option<u32>,
    pub watchtower_status: Option<String>,
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
    fn render(&self, area: Rect, buf: &mut Buffer, node_state: &mut NodeState, config: &AppConfig) {
        let mut default = CoreLightningWidgetState::default();
        let state = node_state
            .widget_state
            .as_any_mut()
            .downcast_mut::<CoreLightningWidgetState>()
            .unwrap_or(&mut default);

        let alias_text = if config.streamer_mode {
            "****".to_string()
        } else {
            state.alias.clone()
        };
        let mut lines = vec![
            Line::from(vec![
                Span::raw("Height: "),
                Span::styled(
                    crate::format::commas(node_state.height),
                    Style::new().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::raw("Alias: "),
                Span::styled(alias_text, Style::new().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::raw("Channels: "),
                Span::styled(
                    format!(
                        "{} up · {} pend · {} down",
                        crate::format::commas(state.num_active_channels as u64),
                        crate::format::commas(state.num_pending_channels as u64),
                        crate::format::commas(state.num_inactive_channels as u64)
                    ),
                    Style::new().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::raw("Peers: "),
                Span::styled(
                    format!(
                        "{} · {} htlc",
                        crate::format::commas(state.num_peers as u64),
                        crate::format::commas(state.num_pending_htlcs as u64)
                    ),
                    Style::new().fg(Color::White),
                ),
            ]),
        ];
        if let Some(count) = state.num_watchtowers {
            lines.push(Line::from(vec![
                Span::raw("Towers: "),
                Span::styled(
                    crate::format::commas(count as u64),
                    Style::new().fg(Color::White),
                ),
            ]));
        }

        if config.streamer_mode {
            let widget = BlockedParagraph::new(&state.title, node_state.status, lines);
            widget.render(area, buf);
        } else {
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
    num_watchtowers: Option<u32>,
    watchtower_status: Option<String>,
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

    async fn fetch_watchtowers(&self) -> Result<(Option<u32>, Option<String>)> {
        let url = format!("{}/v1/listtowers", self.rest_address);
        let response = self
            .client
            .post(&url)
            .header("Rune", &self.rune)
            .header("Content-Type", "application/json")
            .body("{}")
            .send()
            .await?;

        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Ok((None, Some("endpoint not exposed".to_string())));
        }
        if !status.is_success() {
            return Ok((None, Some(format!("HTTP {}", status.as_u16()))));
        }

        let value = response.json::<Value>().await?;
        let count = parse_watchtower_count(&value);
        let status = if count.is_some() {
            None
        } else {
            Some("unexpected response".to_string())
        };
        Ok((count, status))
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
                    num_watchtowers: None,
                    watchtower_status: None,
                });
            }
        };

        let (total_capacity, local_balance, num_pending_htlcs, channel_counts, message) =
            match self.fetch_channels().await {
                Ok(peers) => {
                    let summary = summarize_channels(&peers.channels);
                    (
                        summary.capacity_sat,
                        summary.local_sat,
                        summary.pending_htlcs,
                        Some((summary.up, summary.pending, summary.down)),
                        String::new(),
                    )
                }
                Err(e) => (0, 0, 0, None, format!("Channels fetch error: {}", e)),
            };
        let (num_active_channels, num_pending_channels, num_inactive_channels) = channel_counts
            .unwrap_or((
                info.num_active_channels,
                info.num_pending_channels,
                info.num_inactive_channels,
            ));
        let (num_watchtowers, watchtower_status) = match self.fetch_watchtowers().await {
            Ok((count, status)) => (count, status),
            Err(e) => (None, Some(format!("request {}", e))),
        };

        Ok(NodeInfo {
            status: NodeStatus::Online,
            message,
            height: info.blockheight,
            alias: info.alias,
            num_peers: info.num_peers,
            num_pending_channels,
            num_active_channels,
            num_inactive_channels,
            total_capacity,
            local_balance,
            num_pending_htlcs,
            num_watchtowers,
            watchtower_status,
        })
    }

    async fn update_node_state(&self, sender: UnboundedSender<Event>, index: usize) -> Result<()> {
        let node_info = self.get_node_info().await?;

        let _ = sender.send(Event::NodeUpdate(
            index,
            Arc::new(move |mut state| {
                let title = state
                    .widget_state
                    .as_any()
                    .downcast_ref::<CoreLightningWidgetState>()
                    .map(|widget_state| widget_state.title.clone())
                    .filter(|title| !title.is_empty())
                    .unwrap_or_else(|| "Core Lightning".to_string());

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
                    title,
                    alias: node_info.alias.clone(),
                    num_peers: node_info.num_peers,
                    num_pending_channels: node_info.num_pending_channels,
                    num_active_channels: node_info.num_active_channels,
                    num_inactive_channels: node_info.num_inactive_channels,
                    total_capacity: node_info.total_capacity,
                    local_balance: node_info.local_balance,
                    num_pending_htlcs: node_info.num_pending_htlcs,
                    num_watchtowers: node_info.num_watchtowers,
                    watchtower_status: node_info.watchtower_status.clone(),
                });

                state
            }),
        ));

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

        let _ = thread.sender.send(Event::NodeUpdate(
            index,
            Arc::new(move |mut state| {
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
            }),
        ));

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

#[cfg(test)]
mod tests {
    use super::{
        parse_msat, summarize_channels, Channel, ChannelSummary, GetInfoResponse, Msat,
        PeerChannelsResponse,
    };

    fn channel(state: &str, connected: Option<bool>, total_msat: u64, to_us_msat: u64) -> Channel {
        Channel {
            state: state.to_string(),
            peer_connected: connected,
            total_msat: Msat::Num(total_msat),
            to_us_msat: Msat::Num(to_us_msat),
            htlcs: Vec::new(),
        }
    }

    #[test]
    fn parse_msat_accepts_number_and_unit_strings() {
        assert_eq!(parse_msat("5000000000msat"), 5_000_000_000);
        assert_eq!(parse_msat("5000sat"), 5_000_000);
        assert_eq!(parse_msat("42"), 42);
    }

    #[test]
    fn counts_connected_normal_as_up_and_disconnected_as_down() {
        let summary = summarize_channels(&[
            channel("CHANNELD_NORMAL", Some(true), 1_000_000_000, 400_000_000),
            channel("CHANNELD_NORMAL", Some(true), 2_000_000_000, 500_000_000),
            channel("CHANNELD_NORMAL", Some(false), 3_000_000_000, 100_000_000),
            channel("CHANNELD_AWAITING_LOCKIN", Some(true), 4_000_000_000, 0),
            channel("ONCHAIN", Some(false), 5_000_000_000, 0),
        ]);
        assert_eq!(
            summary,
            ChannelSummary {
                up: 2,
                pending: 1,
                down: 2,
                capacity_sat: 6_000_000,
                local_sat: 1_000_000,
                pending_htlcs: 0,
            }
        );
    }

    #[test]
    fn listpeerchannels_json_counts_live_shape() {
        let parsed: PeerChannelsResponse = serde_json::from_str(
            r#"{
              "channels": [
                {
                  "state": "CHANNELD_NORMAL",
                  "peer_connected": true,
                  "total_msat": 5000000000,
                  "to_us_msat": 1000000000,
                  "htlcs": [{}]
                },
                {
                  "state": "CHANNELD_NORMAL",
                  "peer_connected": false,
                  "total_msat": "2000000000msat",
                  "to_us_msat": "500000000msat"
                }
              ]
            }"#,
        )
        .unwrap();
        assert_eq!(
            summarize_channels(&parsed.channels),
            ChannelSummary {
                up: 1,
                pending: 0,
                down: 1,
                capacity_sat: 7_000_000,
                local_sat: 1_500_000,
                pending_htlcs: 1,
            }
        );
    }

    #[test]
    fn getinfo_defaults_missing_channel_counts() {
        let info: GetInfoResponse =
            serde_json::from_str(r#"{"alias":"test","blockheight":800000}"#).unwrap();
        assert_eq!(info.num_active_channels, 0);
        assert_eq!(info.num_peers, 0);
    }
}
