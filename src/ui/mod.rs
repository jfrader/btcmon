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
    fn draw_status(&self, frame: &mut Frame, area: Rect);
}

pub fn render(config: &AppConfig, state: &AppState, frame: &mut Frame) {
    let node_state = state.node.clone().unwrap_or_default();
    let node = node_state.lock().unwrap();
    let status_style = get_status_style(&node.status);

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Length(frame.size().height / 2),
            Constraint::Length(frame.size().height / 2 - 1),
            Constraint::Max(1),
        ])
        .split(frame.size());

    let top_panel = &main_layout[0];

    let bottom_panel = &main_layout[1];
    let bottom_panel_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(*bottom_panel);

    let bottom_panel_left = &bottom_panel_layout[0];
    let bottom_panel_right = &bottom_panel_layout[1];

    let status_panel = &main_layout[2];

    if config.price.enabled {
        state
            .price
            .draw(frame, *bottom_panel_right, Some(status_style));
    } else {
        state.price.draw(frame, *bottom_panel, Some(status_style));
    }

    state
        .fees
        .draw(frame, *bottom_panel_left, Some(status_style));

    node.draw(frame, *top_panel, Some(status_style));

    node.draw_status(frame, *status_panel);
}

pub fn get_status_style(status: &NodeStatus) -> Style {
    match status {
        NodeStatus::Online => Style::default().fg(Color::Green).bg(Color::Black),
        NodeStatus::Offline => Style::default().fg(Color::Red).bg(Color::Black),
        NodeStatus::Synchronizing => Style::default().fg(Color::Blue).bg(Color::Black),
    }
}
