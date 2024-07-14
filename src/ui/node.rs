use ratatui::prelude::Stylize;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Padding, Paragraph},
};

use crate::node::{NodeState, NodeStatus};

use super::{get_status_style, Draw};

impl Draw for NodeState {
    fn draw(&self, frame: &mut ratatui::Frame, area: Rect, style: Option<Style>) {
        let style = style.unwrap_or(get_status_style(&self.status));

        let block_height = match self.status {
            NodeStatus::Synchronizing => Line::from(vec![
                Span::raw("Block Height: "),
                Span::styled(
                    self.height.to_string(),
                    Style::new().fg(Color::White).italic(),
                ),
                Span::raw("/"),
                Span::styled(
                    self.headers.to_string(),
                    Style::new().fg(Color::Blue).italic(),
                ),
            ]),
            _ => Line::from(vec![
                Span::raw("Block Height: "),
                Span::styled(
                    self.height.to_string(),
                    Style::new().fg(Color::White).italic(),
                ),
            ]),
        };

        let text: Vec<Line> = vec![
            block_height,
            Line::from(vec![
                Span::raw("Last Block: "),
                Span::styled(
                    self.last_hash.clone(),
                    Style::new().fg(Color::White).italic(),
                ),
            ]),
            "------".into(),
        ];

        frame.render_widget(
            Paragraph::new(text)
                .block(
                    Block::bordered()
                        .padding(Padding::left(1))
                        .title("Bitcoin Core")
                        .title_alignment(Alignment::Center)
                        .border_type(BorderType::Plain),
                )
                .style(style),
            area,
        );
    }
}
