// node/providers/core_lightning.rs

use super::super::{AppConfig, AppThread, NodeProvider, NodeStatus};
use crate::event::Event;
use crate::ui::get_status_style;
use crate::widget::{DynamicState, DynamicStatefulWidget};
use anyhow::Result;
use async_trait::async_trait;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding, Paragraph, Widget};
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
    pub height: u64,
    pub alias: String,
    pub num_channels: u64,
    pub status: NodeStatus,
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

impl DynamicStatefulWidget for CoreLightningWidget {
    fn render_dynamic(&self, area: Rect, buf: &mut Buffer, state: &mut dyn DynamicState) {
        let mut default = CoreLightningWidgetState::default();
        let state = state
            .as_any_mut()
            .downcast_mut::<CoreLightningWidgetState>()
            .unwrap_or(&mut default);

        let style = get_status_style(&state.status);
        let text = vec![
            Line::from(vec![
                Span::raw("Block Height: "),
                Span::styled(state.height.to_string(), Style::new().fg(Color::White)),
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
            "------".into(),
        ];

        Paragraph::new(text)
            .block(
                Block::bordered()
                    .padding(Padding::left(1))
                    .title(state.title.clone())
                    .title_alignment(Alignment::Center)
                    .border_type(BorderType::Plain)
                    .style(style),
            )
            .render(area, buf);
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

        let (status, message, height, alias, num_channels) = match info_resp {
            Ok(resp) => {
                let status_code = resp.status();
                if !status_code.is_success() {
                    (
                        NodeStatus::Offline,
                        format!("CLN REST HTTP error: {}", status_code),
                        0,
                        String::new(),
                        0,
                    )
                } else {
                    let info = resp.json::<GetInfoResponse>().await?;
                    let num_channels = match self.get_channels().await {
                        Ok(num) => num,
                        Err(_) => 0,
                    };
                    (
                        NodeStatus::Online,
                        String::new(),
                        info.blockheight,
                        info.alias,
                        num_channels,
                    )
                }
            }
            Err(e) => (
                NodeStatus::Offline,
                format!("Request error: {}", e),
                0,
                String::new(),
                0,
            ),
        };

        let _ = sender.send(Event::NodeUpdate(Arc::new(move |mut state| {
            if state.height > 0 && state.height < height {
                state.last_hash_instant = Some(Instant::now());
            }
            state.status = status;
            state.message = message.clone();
            state.height = height;
            state.last_hash = "N/A".to_string();
            state.alias = alias.clone();
            *state.services.entry("REST".to_string()).or_insert(status) = status;
            state.widget_state = Box::new(CoreLightningWidgetState {
                title: state.title.clone(),
                height,
                alias: alias.clone(),
                num_channels,
                status,
            });

            state
        })));

        if status == NodeStatus::Offline {
            return Err(anyhow::anyhow!("Node info fetch failed"));
        }
        Ok(())
    }

    async fn get_channels(&self) -> Result<u64> {
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
        let num_channels = peers
            .peers
            .iter()
            .flat_map(|peer| &peer.channels)
            .filter(|channel| channel.state == "CHANNELD_NORMAL")
            .count() as u64;

        Ok(num_channels)
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
                state.title = "Core Lightning".to_string();
                state.message = "Initializing CLN REST...".to_string();
                state
                    .services
                    .insert("REST".to_string(), NodeStatus::Offline);
                state.widget_state = Box::new(CoreLightningWidgetState {
                    title: "Core Lightning".to_string(),
                    height: 0,
                    alias: String::new(),
                    num_channels: 0,
                    status: NodeStatus::Offline,
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

    fn widget(&self) -> Box<dyn DynamicStatefulWidget> {
        Box::new(CoreLightningWidget)
    }

    fn widget_state(&self) -> Box<dyn DynamicState> {
        Box::new(CoreLightningWidgetState::default())
    }
}
