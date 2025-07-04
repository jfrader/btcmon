use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Padding, Paragraph, StatefulWidget, Widget};
use throbber_widgets_tui::Throbber;

use super::get_status_style; // Adjust based on your module structure
use crate::node::{NodeState, NodeStatus};

pub struct NodeStatusWidget;

impl StatefulWidget for NodeStatusWidget {
    type State = NodeState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let zmq_status_width = 24;
        let status_bar_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Length(1),
                Constraint::Length(area.width.saturating_sub(zmq_status_width + 1)),
                Constraint::Length(zmq_status_width),
            ])
            .split(area);

        // Throbber or empty block
        if state.status == NodeStatus::Synchronizing {
            let throbber =
                Throbber::default().throbber_set(throbber_widgets_tui::QUADRANT_BLOCK_CRACK);
            Widget::render(throbber, status_bar_layout[0], buf);
        } else {
            Block::new()
                .style(Style::default().fg(Color::White))
                .render(status_bar_layout[0], buf);
        }

        // Status message
        let message = if state.message.is_empty() {
            &state.host
        } else {
            &state.message
        };
        Paragraph::new(format!("Node {} | {}", state.status, message))
            .block(Block::new().padding(Padding::left(1)))
            .style(Style::default().fg(Color::White))
            .render(status_bar_layout[1], buf);

        // Service status
        let keys: Vec<_> = state.services.keys().cloned().collect();
        if !keys.is_empty() {
            let current_key = &keys[state.service_display_index];
            let status = state
                .services
                .get(current_key)
                .unwrap_or(&NodeStatus::Offline);
            Paragraph::new(format!("{} {:?}", current_key, status))
                .style(get_status_style(status))
                .alignment(Alignment::Right)
                .render(status_bar_layout[2], buf);
        }
    }
}
