// ui/mod.rs

use crate::{app::AppState, config::AppConfig, node::NodeStatus};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    Frame,
};

pub mod fees;
pub mod node;
pub mod price;

pub trait Draw {
    fn draw(&self, frame: &mut Frame, area: Rect, style: Option<Style>);
}

pub trait DrawStatus {
    fn draw_status(&mut self, frame: &mut Frame, area: Rect);
}

pub fn render(config: &AppConfig, state: &mut AppState, frame: &mut Frame) {
    let mut node_state = state.node.clone();
    let status_style = get_status_style(&node_state.status);

    let (layout_constraints, status_panel_i): (Vec<Constraint>, usize) =
        if config.price.enabled | config.fees.enabled {
            (
                vec![
                    Constraint::Length(frame.size().height / 2),
                    Constraint::Length(frame.size().height / 2 - 1),
                    Constraint::Max(1),
                ],
                2,
            )
        } else {
            (
                vec![
                    Constraint::Length(frame.size().height - 1),
                    Constraint::Max(1),
                ],
                1,
            )
        };

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(layout_constraints)
        .split(frame.size());

    let status_panel = &main_layout[status_panel_i];
    let top_panel = &main_layout[0];
    let bottom_panel = &main_layout[1];
    let bottom_panel_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(*bottom_panel);

    match (config.price.enabled, config.fees.enabled) {
        (true, true) => {
            let bottom_panel_left = &bottom_panel_layout[0];
            let bottom_panel_right = &bottom_panel_layout[1];

            state
                .price
                .draw(frame, *bottom_panel_right, Some(status_style));
            state
                .fees
                .draw(frame, *bottom_panel_left, Some(status_style));
        }
        (true, false) => {
            state.price.draw(frame, *bottom_panel, Some(status_style));
        }
        (false, true) => {
            state.fees.draw(frame, *bottom_panel, Some(status_style));
        }
        _ => {}
    }

    state.widget.render_dynamic(*top_panel, frame.buffer_mut(), &mut *state.widget_state);
    node_state.widget_state = state.widget_state.clone_box();

    node_state.draw_status(frame, *status_panel);

    if let Some(time) = node_state.last_hash_instant {
        if time.elapsed().as_secs() < 15 && node_state.status == NodeStatus::Online {
            node_state.draw_new_block_popup(frame, node_state.height);
        }
    }
}

pub fn get_status_style(status: &NodeStatus) -> Style {
    match status {
        NodeStatus::Online => Style::default().fg(Color::Green),
        NodeStatus::Offline => Style::default().fg(Color::Red),
        NodeStatus::Synchronizing => Style::default().fg(Color::Yellow),
        NodeStatus::Connecting => Style::default().fg(Color::Blue),
    }
}