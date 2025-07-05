// ui/mod.rs

use crate::{
    app::AppState,
    config::AppConfig,
    node::NodeStatus,
    ui::{
        fees::FeesWidget,
        node::NodeStatusWidget,
        price::{PriceWidget, PriceWidgetOptions},
    },
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    Frame,
};

pub mod fees;
pub mod node;
pub mod price;

pub fn render(config: &AppConfig, state: &mut AppState, frame: &mut Frame) {
    let mut node_state = state.node.clone();

    let (layout_constraints, status_panel_i): (Vec<Constraint>, usize) =
        if config.price.enabled | config.fees.enabled {
            (
                vec![
                    Constraint::Length(frame.area().height / 2),
                    Constraint::Length(frame.area().height / 2 - 1),
                    Constraint::Max(1),
                ],
                2,
            )
        } else {
            (
                vec![
                    Constraint::Length(frame.area().height - 1),
                    Constraint::Max(1),
                ],
                1,
            )
        };

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(layout_constraints)
        .split(frame.area());

    let status_panel = &main_layout[status_panel_i];
    let top_panel = &main_layout[0];
    let bottom_panel = &main_layout[1];
    let bottom_panel_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(*bottom_panel);

    let price_widget = PriceWidget::new(PriceWidgetOptions {
        big_text: config.price.big_text,
    });

    let fees_widget = FeesWidget;

    match (config.price.enabled, config.fees.enabled) {
        (true, true) => {
            let bottom_panel_left = &bottom_panel_layout[0];
            let bottom_panel_right = &bottom_panel_layout[1];

            frame.render_stateful_widget(price_widget, *bottom_panel_right, state);
            frame.render_stateful_widget(fees_widget, *bottom_panel_left, state);
        }
        (true, false) => {
            frame.render_stateful_widget(price_widget, *bottom_panel, state);
        }
        (false, true) => {
            frame.render_stateful_widget(fees_widget, *bottom_panel, state);
        }
        _ => {}
    }

    state
        .widget
        .render(*top_panel, frame.buffer_mut(), &mut node_state);

    frame.render_stateful_widget(NodeStatusWidget, *status_panel, &mut node_state);

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
