use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::{Block, Padding, Paragraph},
};

use crate::node::{NodeState, NodeStatus};

use super::{get_status_style, DrawStatus};

impl DrawStatus for NodeState {
    fn draw_status(&mut self, frame: &mut Frame, area: Rect) {
        let zmq_status_width = 24;
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
                Block::new().style(Style::default().fg(Color::White)),
                status_bar_layout[0],
            );
        }

        let message = if self.message.is_empty() {
            &self.host
        } else {
            &self.message
        };

        frame.render_widget(
            Paragraph::new(format!("Node {} | {}", self.status, message))
                .block(Block::new().padding(Padding::left(1)))
                .style(Style::default().fg(Color::White)),
            status_bar_layout[1],
        );

        let keys: Vec<_> = self.services.keys().cloned().collect();

        if !keys.is_empty() {
            let current_key = &keys[self.service_display_index];
            let status = self
                .services
                .get(current_key)
                .unwrap_or(&NodeStatus::Offline);

            frame.render_widget(
                Paragraph::new(format!("{} {:?}", current_key, status))
                    .style(get_status_style(status))
                    .alignment(Alignment::Right),
                status_bar_layout[2],
            );
        }
    }
}
