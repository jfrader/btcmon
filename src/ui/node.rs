use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Padding, Paragraph, StatefulWidget, Widget};
use throbber_widgets_tui::Throbber;

use crate::node::{NodeState, NodeStatus};
use crate::ui::get_status_style;

pub struct NodeStatusWidget;

impl StatefulWidget for NodeStatusWidget {
    type State = NodeState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let zmq_status_width = 20;
        let indicator_width = 25; // Width for the combined node status and indicator

        // Adjust layout based on the number of nodes
        let constraints = if state.total_nodes > 1 {
            vec![
                Constraint::Length(1),                // Throbber/empty block
                Constraint::Length(zmq_status_width), // Service status
                Constraint::Length(
                    area.width
                        .saturating_sub(zmq_status_width + indicator_width + 1),
                ), // Placeholder
                Constraint::Length(indicator_width),  // Combined node status and indicator
            ]
        } else {
            vec![
                Constraint::Length(1),                // Throbber/empty block
                Constraint::Length(zmq_status_width), // Service status
                Constraint::Length(area.width.saturating_sub(zmq_status_width + 1)), // Node status (full remaining width)
            ]
        };
        let status_bar_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
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
                .alignment(Alignment::Left)
                .render(status_bar_layout[1], buf);
        }

        if state.total_nodes > 1 {
            // Placeholder for the old status message area (can be empty or removed)
            Block::new()
                .style(Style::default().fg(Color::Black))
                .render(status_bar_layout[2], buf);

            // Combined node status and indicator (only for multiple nodes)
            let current_node = state.current_node_index + 1; // 1-based index
            let total_nodes = state.total_nodes;
            let seconds = state.seconds_until_rotation;
            let indicator_text = format!(
                "Node {}/{} {} ({}s)",
                current_node, total_nodes, state.status, seconds
            );
            Paragraph::new(indicator_text)
                .style(Style::default().fg(Color::White))
                .alignment(Alignment::Right)
                .render(status_bar_layout[3], buf);
        } else {
            // For a single node, show only the node status
            let message = if state.message.is_empty() {
                &state.host
            } else {
                &state.message
            };
            Paragraph::new(format!("Node {} | {}", state.status, message))
                .block(Block::new().padding(Padding::left(1)))
                .style(Style::default().fg(Color::White))
                .alignment(Alignment::Right)
                .render(status_bar_layout[2], buf);
        }
    }
}
