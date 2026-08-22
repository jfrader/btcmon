use anyhow::Result;
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
use crate::config::{AppConfig, LndSettings};
use crate::event::Event;
use crate::node::widgets::{BlockedParagraph, BlockedParagraphWithGauge};
use crate::node::{NodeProvider, NodeState, NodeStatus};
use crate::widget::{DynamicNodeStatefulWidget, DynamicState};

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
                .and_then(|towers| {
                    towers
                        .get("towers")
                        .and_then(|nested| nested.as_array())
                        .map(|nested| nested.len() as u32)
                        .or_else(|| Some(towers.len() as u32))
                })
        })
        .or_else(|| {
            value
                .get("tower")
                .and_then(|towers| towers.as_array())
                .map(|towers| towers.len() as u32)
        })
        .or_else(|| {
            value
                .get("num_towers")
                .and_then(|count| count.as_u64())
                .map(|count| count as u32)
        })
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
    pub num_watchtowers: Option<u32>,
    pub watchtower_status: Option<String>,
    pub watchtower_server_online: Option<bool>,
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
        let mut lines = Vec::new();
        lines.push(block_height);
        lines.push(Line::from(vec![
            Span::raw("Alias: "),
            Span::styled(alias_text, Style::new().fg(Color::White)),
        ]));
        lines.push(Line::from(vec![
            Span::raw("Active Channels: "),
            Span::styled(
                state.num_active_channels.to_string(),
                Style::new().fg(Color::White),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("Pending Channels: "),
            Span::styled(
                state.num_pending_channels.to_string(),
                Style::new().fg(Color::White),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("Inactive Channels: "),
            Span::styled(
                state.num_inactive_channels.to_string(),
                Style::new().fg(Color::White),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("Peers: "),
            Span::styled(state.num_peers.to_string(), Style::new().fg(Color::White)),
        ]));
        if let Some(count) = state.num_watchtowers {
            lines.push(Line::from(vec![
                Span::raw("Connected Towers: "),
                Span::styled(count.to_string(), Style::new().fg(Color::White)),
            ]));
        }
        lines.push(Line::from(vec![
            Span::raw("Pending HTLCs: "),
            Span::styled(
                state.num_pending_htlcs.to_string(),
                Style::new().fg(Color::White),
            ),
        ]));
        if let Some(is_online) = state.watchtower_server_online {
            let status = if is_online { "Online" } else { "Offline" };
            lines.push(Line::from(vec![
                Span::raw("Tower Server: "),
                Span::styled(status, Style::new().fg(Color::White)),
            ]));
        }
        lines.push(Line::from(vec![
            Span::raw("Synced to Bitcoin: "),
            Span::styled(
                if state.synced_to_chain {
                    "True"
                } else {
                    "False"
                },
                Style::new().fg(Color::White),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("Synced to Lightning: "),
            Span::styled(
                if state.synced_to_graph {
                    "True"
                } else {
                    "False"
                },
                Style::new().fg(Color::White),
            ),
        ]));
        lines.push(Line::raw(""));

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

    fn build_url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.address.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    async fn get_channels(&self) -> Result<ChannelsResponse> {
        let url = self.build_url("/v1/channels");
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

    async fn get_watchtower_count(&self) -> Result<(Option<u32>, Option<String>)> {
        let endpoints = [
            "/v2/watchtower/client/towers",
            "/v2/watchtower/client",
            "/v1/watchtower/client/towers",
            "/v1/watchtower/client",
        ];
        let mut saw_not_found = false;
        let mut last_status: Option<String> = None;

        for endpoint in endpoints {
            let url = self.build_url(endpoint);
            let resp = self
                .client
                .get(&url)
                .header("Grpc-Metadata-macaroon", &self.macaroon)
                .send()
                .await?;

            let status = resp.status();
            if status == StatusCode::NOT_FOUND {
                saw_not_found = true;
                continue;
            }
            if !status.is_success() {
                last_status = Some(format!("HTTP {}", status.as_u16()));
                continue;
            }

            let value = resp.json::<Value>().await?;
            let count = parse_watchtower_count(&value);
            if let Some(count) = count {
                return Ok((Some(count), None));
            }
            last_status = Some("unexpected response".to_string());
        }

        if saw_not_found {
            return Ok((None, Some("endpoint not exposed".to_string())));
        }

        Ok((None, last_status))
    }

    async fn get_watchtower_server_status(&self) -> Result<Option<bool>> {
        let endpoints = ["/v2/watchtower/server", "/v1/watchtower/server"];

        for endpoint in endpoints {
            let url = self.build_url(endpoint);
            let resp = self
                .client
                .get(&url)
                .header("Grpc-Metadata-macaroon", &self.macaroon)
                .send()
                .await?;

            let status = resp.status();
            if status == StatusCode::NOT_FOUND || status == StatusCode::NOT_IMPLEMENTED {
                continue;
            }
            if status.is_success() {
                return Ok(Some(true));
            }
            return Ok(Some(false));
        }

        Ok(None)
    }

    async fn get_node_info(&self, sender: UnboundedSender<Event>, index: usize) -> Result<()> {
        let url = self.build_url("/v1/getinfo");

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
                let (num_watchtowers, watchtower_status) = match self.get_watchtower_count().await {
                    Ok((count, status)) => (count, status),
                    Err(e) => (None, Some(format!("request {}", e))),
                };
                let watchtower_server_online = match self.get_watchtower_server_status().await {
                    Ok(status) => status,
                    Err(_) => None,
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
                            num_watchtowers,
                            watchtower_status: watchtower_status.clone(),
                            watchtower_server_online,
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
                    num_watchtowers: None,
                    watchtower_status: None,
                    watchtower_server_online: None,
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
