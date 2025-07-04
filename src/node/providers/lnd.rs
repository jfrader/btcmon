// node/providers/lnd.rs

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
    pub block_height: u64,
    pub alias: String,
    pub num_active_channels: u64, // LND API field
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
    pub height: u64,
    pub alias: String,
    pub num_channels: u64, // Internal state for open channels
    pub status: NodeStatus,
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

impl DynamicStatefulWidget for LndWidget {
    fn render_dynamic(&self, area: Rect, buf: &mut Buffer, state: &mut dyn DynamicState) {
        let mut default = LndWidgetState::default();
        let state = state
            .as_any_mut()
            .downcast_mut::<LndWidgetState>()
            .unwrap_or(&mut default);

        let style = get_status_style(&state.status);
        let text = vec![
            Line::from(vec![
                Span::raw("Block Height: "),
                Span::styled(state.height.to_string(), Style::new().fg(Color::White)),
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
                    .title(vec![state.title.clone(), state.alias.clone()].join(" | "))
                    .title_alignment(Alignment::Center)
                    .border_type(BorderType::Plain)
                    .style(style),
            )
            .render(area, buf);
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
                        let num_channels = state
                            .widget_state
                            .as_any()
                            .downcast_ref::<LndWidgetState>()
                            .map_or(0, |s| s.num_channels);
                        state.message = format!("LND REST error: HTTP {}", status);
                        state.widget_state = Box::new(LndWidgetState {
                            title: state.title.clone(),
                            height: state.height,
                            alias: state.alias.clone(),
                            num_channels, // Preserve from previous state
                            status: NodeStatus::Offline,
                        });
                        state
                    })));
                    return Err(anyhow::anyhow!("LND REST non-200: {}", status));
                }

                let info = resp.json::<GetInfoResponse>().await?;

                let _ = sender.send(Event::NodeUpdate(Arc::new(move |mut state| {
                    if state.height > 0 && state.height < info.block_height {
                        state.last_hash_instant = Some(Instant::now());
                    }
                    state.message = "".to_string();
                    state.status = NodeStatus::Online;
                    state.height = info.block_height;
                    state.last_hash = "N/A".to_string();
                    state.alias = info.alias.clone();
                    *state
                        .services
                        .entry("REST".to_string())
                        .or_insert(NodeStatus::Online) = NodeStatus::Online;
                    state.widget_state = Box::new(LndWidgetState {
                        title: state.title.clone(),
                        height: info.block_height,
                        alias: info.alias.clone(),
                        num_channels: info.num_active_channels,
                        status: NodeStatus::Online,
                    });
                    state
                })));

                Ok(())
            }
            Err(e) => {
                let _ = sender.send(Event::NodeUpdate(Arc::new(move |mut state| {
                    let num_channels = state
                        .widget_state
                        .as_any()
                        .downcast_ref::<LndWidgetState>()
                        .map_or(0, |s| s.num_channels);
                    state.widget_state = Box::new(LndWidgetState {
                        title: state.title.clone(),
                        height: state.height,
                        alias: state.alias.clone(),
                        num_channels, // Preserve from previous state
                        status: NodeStatus::Offline,
                    });
                    state
                })));
                Err(anyhow::anyhow!("Request error: {:?}", e))
            }
        }
    }

    async fn check_node_status(&self, sender: UnboundedSender<Event>) -> Result<()> {
        match self.get_node_info(sender.clone()).await {
            Ok(_) => Ok(()),
            Err(e) => {
                let _ = sender.send(Event::NodeUpdate(Arc::new(|mut state| {
                    let num_channels = state
                        .widget_state
                        .as_any()
                        .downcast_ref::<LndWidgetState>()
                        .map_or(0, |s| s.num_channels);
                    state.status = NodeStatus::Offline;
                    *state
                        .services
                        .entry("REST".to_string())
                        .or_insert(NodeStatus::Offline) = NodeStatus::Offline;
                    state.widget_state = Box::new(LndWidgetState {
                        title: state.title.clone(),
                        height: state.height,
                        alias: state.alias.clone(),
                        num_channels, // Preserve from previous state
                        status: NodeStatus::Offline,
                    });
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

        let _ = thread.sender.send(Event::NodeUpdate(Arc::new(|mut state| {
            state.title = "LND".to_string();
            state.message = "Initializing LND REST...".to_string();
            state
                .services
                .insert("REST".to_string(), NodeStatus::Offline);
            state.widget_state = Box::new(LndWidgetState {
                title: "LND".to_string(),
                height: 0,
                alias: "".to_string(),
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
        Box::new(LndWidget)
    }

    fn widget_state(&self) -> Box<dyn DynamicState> {
        Box::new(LndWidgetState::default())
    }
}
