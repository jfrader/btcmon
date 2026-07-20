use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Paragraph, StatefulWidget, Widget};
use throbber_widgets_tui::Throbber;

use crate::node::{NodeState, NodeStatus};
use crate::ui::get_status_style;

pub struct NodeStatusWidget;

impl StatefulWidget for NodeStatusWidget {
    type State = NodeState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let status_bar_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(1),
                Constraint::Percentage(42),
                Constraint::Min(0),
            ])
            .split(area);

        if state.status == NodeStatus::Synchronizing {
            let throbber =
                Throbber::default().throbber_set(throbber_widgets_tui::QUADRANT_BLOCK_CRACK);
            Widget::render(throbber, status_bar_layout[0], buf);
        } else {
            Block::new()
                .style(Style::default().fg(Color::White))
                .render(status_bar_layout[0], buf);
        }

        let mut keys: Vec<_> = state.services.keys().cloned().collect();
        keys.sort();
        if !keys.is_empty() {
            let current_key = &keys[state.service_display_index % keys.len()];
            let status = state
                .services
                .get(current_key)
                .unwrap_or(&NodeStatus::Offline);
            Paragraph::new(format!("{} {:?}", current_key, status))
                .style(get_status_style(status))
                .alignment(Alignment::Left)
                .render(status_bar_layout[1], buf);
        }

        let context = if state.message.is_empty() {
            state.host.as_str()
        } else {
            state.message.as_str()
        };
        let status_text = if context.is_empty() {
            state.status.to_string()
        } else {
            format!("{} | {}", state.status, context)
        };
        Paragraph::new(status_text)
            .style(get_status_style(&state.status))
            .alignment(Alignment::Right)
            .render(status_bar_layout[2], buf);
    }
}
