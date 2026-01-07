use crate::{
    app::App,
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
use tokio::time::Instant;

pub mod fees;
pub mod node;
pub mod price;

pub fn render(config: &AppConfig, app: &mut App, frame: &mut Frame) {
    if app.nodes.is_empty() || app.state.node_states.is_empty() {
        let price_widget = PriceWidget::new(PriceWidgetOptions {
            big_text: config.price.big_text,
            style: Style::default(),
            pixel_size: tui_widgets::big_text::PixelSize::Full,
            price_style: get_price_style(config, &app.state),
            title: "Bitcoin Price".to_string(),
        });
        frame.render_stateful_widget(price_widget, frame.area(), &mut app.state);
        return;
    }

    let current_index = app.current_node_index;

    let (layout_constraints, status_panel_i): (Vec<Constraint>, usize) =
        if config.price.enabled || config.fees.enabled {
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

    let node_status = app.state.node_states[current_index].status;
    let style = get_status_style(&node_status);

    let price_widget = PriceWidget::new(PriceWidgetOptions {
        big_text: config.price.big_text,
        style,
        pixel_size: tui_widgets::big_text::PixelSize::Sextant,
        price_style: get_price_style(config, &app.state),
        title: "Price".to_string(),
    });

    let fees_widget = FeesWidget { style };

    match (config.price.enabled, config.fees.enabled) {
        (true, true) => {
            let bottom_panel_left = &bottom_panel_layout[0];
            let bottom_panel_right = &bottom_panel_layout[1];

            frame.render_stateful_widget(price_widget, *bottom_panel_right, &mut app.state);
            frame.render_stateful_widget(fees_widget, *bottom_panel_left, &mut app.state);
        }
        (true, false) => {
            frame.render_stateful_widget(price_widget, *bottom_panel, &mut app.state);
        }
        (false, true) => {
            frame.render_stateful_widget(fees_widget, *bottom_panel, &mut app.state);
        }
        _ => {}
    }

    app.widgets[current_index].render(
        *top_panel,
        frame.buffer_mut(),
        &mut app.state.node_states[current_index],
        &app.config,
    );

    // Use only NodeState for NodeStatusWidget
    let mut state = app.state.node_states[current_index].clone();
    frame.render_stateful_widget(NodeStatusWidget, *status_panel, &mut state);

    let node_state = &app.state.node_states[current_index];
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

fn get_price_style(config: &AppConfig, state: &crate::app::AppState) -> Style {
    let Some(current) = state.price.last_price_in_currency else {
        return Style::default().fg(Color::White);
    };

    let window = match config.price.variation.as_str() {
        "minute" => std::time::Duration::from_secs(60),
        "hour" => std::time::Duration::from_secs(60 * 60),
        "day" => std::time::Duration::from_secs(60 * 60 * 24),
        _ => std::time::Duration::from_secs(60),
    };

    let now = Instant::now();
    let cutoff = now.checked_sub(window).unwrap_or(now);
    let mut reference = None;
    for (timestamp, price) in state.price_history.iter() {
        if *timestamp <= cutoff {
            reference = Some(*price);
        } else {
            break;
        }
    }
    if reference.is_none() {
        reference = state.price_history.front().map(|(_, price)| *price);
    }

    let Some(reference) = reference else {
        return Style::default().fg(Color::White);
    };
    if reference == 0.0 {
        return Style::default().fg(Color::White);
    }

    let change_pct = (current - reference) / reference * 100.0;
    let threshold = config.price.variation_threshold.abs();
    if change_pct.abs() < threshold {
        Style::default().fg(Color::White)
    } else if change_pct > 0.0 {
        Style::default().fg(Color::Green)
    } else if change_pct < 0.0 {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::White)
    }
}
