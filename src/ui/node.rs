use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::Stylize;
use ratatui::Frame;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Padding, Paragraph},
};
use tui_popup::{Popup, SizedWrapper};

use crate::node::{NodeState, NodeStatus};

use super::{get_status_style, Draw, DrawStatus};

impl NodeState {
    fn draw_new_block_popup(&self, frame: &mut Frame, block_height: u64) {
        let sized_paragraph = SizedWrapper {
            inner: Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![Span::raw("Height")]),
                Line::from(vec![Span::raw(block_height.to_string())]),
                Line::from(""),
            ])
            .centered(),
            width: 21,
            height: 4,
        };

        let popup = Popup::new(" New block! ", sized_paragraph)
            .style(Style::new().fg(Color::White).bg(Color::Black));
        frame.render_widget(&popup, frame.size());
    }
}

impl DrawStatus for NodeState {
    fn draw_status(&self, frame: &mut Frame, area: Rect) {
        let zmq_status_width = 12;
        let status_bar_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Length(1),
                Constraint::Length(frame.size().width - zmq_status_width - 1),
                Constraint::Length(zmq_status_width),
            ])
            .split(area);

        if self.status == NodeStatus::Synchronizing {
            let throbber = throbber_widgets_tui::Throbber::default()
                .throbber_set(throbber_widgets_tui::QUADRANT_BLOCK_CRACK);
            frame.render_widget(throbber, status_bar_layout[0]);
        } else {
            frame.render_widget(
                Block::new().style(Style::default().fg(Color::White).bg(Color::Black)),
                status_bar_layout[0],
            );
        }

        frame.render_widget(
            Paragraph::new(format!("Node {}", self.status))
                .block(Block::new().padding(Padding::left(1)))
                .style(Style::default().fg(Color::White).bg(Color::Black)),
            status_bar_layout[1],
        );

        frame.render_widget(
            Paragraph::new(format!(
                "ZMQ {:?} ",
                self.services.get("ZMQ").unwrap_or(&NodeStatus::Offline)
            ))
            .style(Style::default().fg(Color::White).bg(Color::Black))
            .right_aligned(),
            status_bar_layout[2],
        );
    }
}

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

        if let Some(time) = self.last_hash_instant {
            if time.elapsed().as_secs() < 15 && self.status == NodeStatus::Online {
                self.draw_new_block_popup(frame, self.height);
            }
        }
    }
}
