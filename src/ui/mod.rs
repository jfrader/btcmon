use crate::{
    app::{App, DashboardView, TouchAction, TouchTarget},
    config::AppConfig,
    node::NodeStatus,
    ui::{
        fees::FeesWidget,
        node::NodeStatusWidget,
        price::{PriceWidget, PriceWidgetOptions},
    },
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Paragraph, Widget},
    Frame,
};
use tokio::time::Instant;
use tui_widgets::big_text::PixelSize;

pub mod fees;
pub mod node;
pub mod price;

const TOUCH_DOCK_HEIGHT: u16 = 3;
const BITCOIN_ORANGE: Color = Color::Rgb(247, 147, 26);

pub fn render(config: &AppConfig, app: &mut App, frame: &mut Frame) {
    app.touch_targets.clear();

    let available_views = app.available_views();
    if !available_views.contains(&app.active_view) {
        app.active_view = available_views[0];
    }

    let show_touch_dock =
        frame.area().height >= 11 && (app.nodes.len() > 1 || available_views.len() > 1);
    let (content_area, dock_area) = if show_touch_dock {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(TOUCH_DOCK_HEIGHT)])
            .split(frame.area());
        (layout[0], Some(layout[1]))
    } else {
        (frame.area(), None)
    };

    match app.active_view {
        DashboardView::Overview => render_overview(config, app, frame, content_area),
        DashboardView::Node => render_node_view(config, app, frame, content_area),
        DashboardView::Price => render_price_view(config, app, frame, content_area),
        DashboardView::Fees => render_fees_view(app, frame, content_area),
    }

    render_new_block_popup(app, frame);

    if let Some(area) = dock_area {
        render_touch_dock(app, frame, area, &available_views);
    }

    if app.view_menu_open && available_views.len() > 1 {
        render_view_menu(app, frame, content_area, &available_views);
    }
}

fn render_overview(config: &AppConfig, app: &mut App, frame: &mut Frame, area: Rect) {
    if app.nodes.is_empty() || app.state.node_states.is_empty() {
        render_price_view(config, app, frame, area);
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(55),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    render_node_summary(app, frame, layout[0]);

    let current_index = app.current_node_index;
    let status = app.state.node_states[current_index].status;
    let status_style = get_status_style(&status);

    match (config.price.enabled, config.fees.enabled) {
        (true, true) => {
            let auxiliary = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(42), Constraint::Min(0)])
                .split(layout[1]);
            frame.render_stateful_widget(
                FeesWidget {
                    style: status_style,
                },
                auxiliary[0],
                &mut app.state,
            );
            render_price_widget(app, frame, auxiliary[1], "Price", PixelSize::Sextant);
        }
        (true, false) => {
            render_price_widget(app, frame, layout[1], "Price", PixelSize::Sextant);
        }
        (false, true) => {
            frame.render_stateful_widget(
                FeesWidget {
                    style: status_style,
                },
                layout[1],
                &mut app.state,
            );
        }
        (false, false) => {}
    }

    render_node_status(app, frame, layout[2]);
}

fn render_node_view(config: &AppConfig, app: &mut App, frame: &mut Frame, area: Rect) {
    if app.nodes.is_empty() || app.state.node_states.is_empty() {
        render_price_view(config, app, frame, area);
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);
    render_current_node(config, app, frame, layout[0]);
    render_node_status(app, frame, layout[1]);
}

fn render_current_node(config: &AppConfig, app: &mut App, frame: &mut Frame, area: Rect) {
    let current_index = app.current_node_index;
    app.widgets[current_index].render(
        area,
        frame.buffer_mut(),
        &mut app.state.node_states[current_index],
        config,
    );
}

fn render_node_summary(app: &App, frame: &mut Frame, area: Rect) {
    let state = &app.state.node_states[app.current_node_index];
    let status_style = get_status_style(&state.status);
    let block = Block::bordered()
        .title(fit_text(
            &app.current_node_name(),
            area.width.saturating_sub(2),
        ))
        .title_alignment(Alignment::Center)
        .border_type(BorderType::Plain)
        .border_style(status_style);
    let inner = block.inner(area);
    block.render(area, frame.buffer_mut());

    let mut services: Vec<_> = state.services.iter().collect();
    services.sort_by(|(left, _), (right, _)| left.cmp(right));
    let mut service_spans = vec![Span::styled(
        "SERVICES  ",
        Style::default().fg(Color::DarkGray),
    )];
    if services.is_empty() {
        service_spans.push(Span::styled("--", Style::default().fg(Color::DarkGray)));
    } else {
        for (index, (name, status)) in services.into_iter().enumerate() {
            if index > 0 {
                service_spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
            }
            service_spans.push(Span::styled(
                format!("{} {}", name, status.to_string().to_uppercase()),
                get_status_style(status),
            ));
        }
    }

    let mut lines = vec![
        Line::from(vec![
            Span::styled("STATUS    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                state.status.to_string().to_uppercase(),
                status_style.add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("HEIGHT    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if state.height == 0 {
                    "--".to_string()
                } else {
                    crate::format::commas(state.height)
                },
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    lines.extend(node_glance_lines(state, app.config.streamer_mode));
    lines.push(Line::from(service_spans));
    if !state.message.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("INFO      ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.message, Style::default().fg(Color::White)),
        ]));
    }

    Paragraph::new(lines).render(inner, frame.buffer_mut());
}

fn render_node_status(app: &App, frame: &mut Frame, area: Rect) {
    let mut state = app.state.node_states[app.current_node_index].clone();
    frame.render_stateful_widget(NodeStatusWidget, area, &mut state);
}

fn render_price_view(config: &AppConfig, app: &mut App, frame: &mut Frame, area: Rect) {
    let spark_height = if area.height >= 12 { 2 } else { 0 };
    let footer_height = if area.height >= 10 { 2 } else { 1 };
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(spark_height),
            Constraint::Length(footer_height),
        ])
        .split(area);
    render_price_widget(app, frame, layout[0], "Bitcoin Price", PixelSize::Full);

    if spark_height > 0 {
        let values: Vec<f64> = app
            .state
            .price_history
            .iter()
            .map(|(_, price)| *price)
            .collect();
        let spark = crate::format::sparkline(&values, layout[1].width as usize);
        Paragraph::new(spark)
            .style(Style::default().fg(BITCOIN_ORANGE))
            .alignment(Alignment::Center)
            .render(layout[1], frame.buffer_mut());
    }

    let (status_text, status_style) = get_price_ticker_footer(config, &app.state);
    Paragraph::new(status_text)
        .style(status_style)
        .alignment(Alignment::Center)
        .render(layout[2], frame.buffer_mut());
}

fn render_price_widget(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    title: &str,
    pixel_size: PixelSize,
) {
    let widget = PriceWidget::new(PriceWidgetOptions {
        big_text: app.config.price.big_text,
        style: get_price_block_style(&app.state),
        pixel_size,
        price_style: Style::default().fg(Color::White),
        title: title.to_string(),
    });
    frame.render_stateful_widget(widget, area, &mut app.state);
}

fn render_fees_view(app: &mut App, frame: &mut Frame, area: Rect) {
    let style = app
        .state
        .node_states
        .get(app.current_node_index)
        .map(|state| get_status_style(&state.status))
        .unwrap_or_else(|| Style::default().fg(Color::White));
    frame.render_stateful_widget(FeesWidget { style }, area, &mut app.state);
}

fn render_new_block_popup(app: &App, frame: &mut Frame) {
    if !matches!(
        app.active_view,
        DashboardView::Overview | DashboardView::Node
    ) {
        return;
    }
    let Some(node_state) = app.state.node_states.get(app.current_node_index) else {
        return;
    };
    if let Some(time) = node_state.last_hash_instant {
        if time.elapsed().as_secs() < 15 && node_state.status == NodeStatus::Online {
            node_state.draw_new_block_popup(frame, node_state.height);
        }
    }
}

fn render_touch_dock(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    available_views: &[DashboardView],
) {
    if app.nodes.len() > 1 {
        let (constraints, node_index, view_index, next_index) = if available_views.len() > 1 {
            (
                vec![
                    Constraint::Percentage(14),
                    Constraint::Percentage(36),
                    Constraint::Percentage(36),
                    Constraint::Percentage(14),
                ],
                1,
                Some(2),
                3,
            )
        } else {
            (
                vec![
                    Constraint::Percentage(18),
                    Constraint::Percentage(64),
                    Constraint::Percentage(18),
                ],
                1,
                None,
                2,
            )
        };
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(area);

        render_dock_button(frame, layout[0], "", "<", false);
        app.touch_targets.push(TouchTarget {
            area: layout[0],
            action: TouchAction::PreviousNode,
        });

        let rotation = if app.auto_rotate_nodes {
            format!("AUTO {}s", app.seconds_until_rotation)
        } else {
            "PINNED".to_string()
        };
        let node_text = format!(
            "{} {}/{}",
            app.current_node_name(),
            app.current_node_index + 1,
            app.nodes.len()
        );
        render_dock_button(
            frame,
            layout[node_index],
            &node_text,
            &rotation,
            app.auto_rotate_nodes,
        );
        app.touch_targets.push(TouchTarget {
            area: layout[node_index],
            action: TouchAction::ToggleNodeRotation,
        });

        if let Some(view_index) = view_index {
            render_dock_button(
                frame,
                layout[view_index],
                "VIEW",
                app.active_view.label(),
                app.view_menu_open,
            );
            app.touch_targets.push(TouchTarget {
                area: layout[view_index],
                action: TouchAction::ToggleViewMenu,
            });
        }

        render_dock_button(frame, layout[next_index], "", ">", false);
        app.touch_targets.push(TouchTarget {
            area: layout[next_index],
            action: TouchAction::NextNode,
        });
        return;
    }

    if !app.nodes.is_empty() {
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(area);
        let state = &app.state.node_states[app.current_node_index];
        render_dock_button(
            frame,
            layout[0],
            &app.current_node_name(),
            &state.status.to_string().to_uppercase(),
            false,
        );
        render_dock_button(
            frame,
            layout[1],
            "VIEW",
            app.active_view.label(),
            app.view_menu_open,
        );
        app.touch_targets.push(TouchTarget {
            area: layout[1],
            action: TouchAction::ToggleViewMenu,
        });
        return;
    }

    render_dock_button(
        frame,
        area,
        "VIEW",
        app.active_view.label(),
        app.view_menu_open,
    );
    app.touch_targets.push(TouchTarget {
        area,
        action: TouchAction::ToggleViewMenu,
    });
}

fn render_view_menu(
    app: &mut App,
    frame: &mut Frame,
    content_area: Rect,
    available_views: &[DashboardView],
) {
    app.touch_targets.push(TouchTarget {
        area: content_area,
        action: TouchAction::DismissViewMenu,
    });

    let row_count = available_views.len().div_ceil(2) as u16;
    let menu_height = (row_count * 4 + 2).min(content_area.height);
    let menu_width = content_area.width.saturating_sub(4).min(52);
    let menu_area = centered_rect(content_area, menu_width, menu_height);

    Clear.render(menu_area, frame.buffer_mut());
    let menu_block = Block::bordered()
        .title(" SELECT VIEW ")
        .title_alignment(Alignment::Center)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(BITCOIN_ORANGE));
    let inner = menu_block.inner(menu_area);
    menu_block.render(menu_area, frame.buffer_mut());

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints((0..row_count).map(|_| Constraint::Ratio(1, row_count.into())))
        .split(inner);

    for (index, view) in available_views.iter().copied().enumerate() {
        let row = index / 2;
        let column = index % 2;
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[row]);
        let button_area = columns[column];
        render_menu_button(
            frame,
            button_area,
            &format!("{}  {}", index + 1, view.label()),
            app.active_view == view,
        );
        app.touch_targets.push(TouchTarget {
            area: button_area,
            action: TouchAction::SelectView(view),
        });
    }
}

fn render_dock_button(frame: &mut Frame, area: Rect, title: &str, label: &str, active: bool) {
    let color = if active { BITCOIN_ORANGE } else { Color::White };
    let block = Block::bordered()
        .title(fit_text(title, area.width.saturating_sub(2)))
        .title_alignment(Alignment::Center)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(color));
    let inner = block.inner(area);
    block.render(area, frame.buffer_mut());
    Paragraph::new(fit_text(label, inner.width))
        .alignment(Alignment::Center)
        .style(Style::default().fg(color).add_modifier(Modifier::BOLD))
        .render(inner, frame.buffer_mut());
}

fn render_menu_button(frame: &mut Frame, area: Rect, label: &str, selected: bool) {
    let style = if selected {
        Style::default()
            .fg(Color::Black)
            .bg(BITCOIN_ORANGE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    };
    Paragraph::new(label)
        .block(
            Block::bordered()
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(BITCOIN_ORANGE)),
        )
        .alignment(Alignment::Center)
        .style(style)
        .render(area, frame.buffer_mut());
}

fn fit_text(text: &str, width: u16) -> String {
    let width = width as usize;
    if text.chars().count() <= width {
        return text.to_string();
    }
    if width <= 1 {
        return text.chars().take(width).collect();
    }
    format!("{}~", text.chars().take(width - 1).collect::<String>())
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
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
    let Some((change_pct, _label)) = get_price_variation(config, state) else {
        return Style::default().fg(Color::White);
    };

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

fn node_glance_lines(state: &crate::node::NodeState, streamer_mode: bool) -> Vec<Line<'_>> {
    use crate::node::providers::bitcoin_core::BitcoinCoreWidgetState;
    use crate::node::providers::core_lightning::CoreLightningWidgetState;
    use crate::node::providers::lnd::LndWidgetState;

    if let Some(core) = state
        .widget_state
        .as_any()
        .downcast_ref::<BitcoinCoreWidgetState>()
    {
        let mut lines = vec![Line::from(vec![
            Span::styled("NETWORK   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "{} · {} peers",
                    crate::format::chain_label(&core.chain),
                    crate::format::commas(core.peers)
                ),
                Style::default().fg(Color::White),
            ),
        ])];
        if core.mempool_tx > 0 {
            lines.push(Line::from(vec![
                Span::styled("MEMPOOL   ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{} tx", crate::format::commas(core.mempool_tx)),
                    Style::default().fg(Color::White),
                ),
            ]));
        }
        return lines;
    }

    let channels = state
        .widget_state
        .as_any()
        .downcast_ref::<LndWidgetState>()
        .map(|ln| {
            (
                ln.num_active_channels,
                ln.num_pending_channels,
                ln.num_inactive_channels,
                ln.local_balance,
                ln.capacity,
            )
        })
        .or_else(|| {
            state
                .widget_state
                .as_any()
                .downcast_ref::<CoreLightningWidgetState>()
                .map(|ln| {
                    (
                        ln.num_active_channels as u64,
                        ln.num_pending_channels as u64,
                        ln.num_inactive_channels as u64,
                        ln.local_balance,
                        ln.total_capacity,
                    )
                })
        });

    let Some((active, pending, inactive, local, capacity)) = channels else {
        return Vec::new();
    };

    let mut lines = vec![Line::from(vec![
        Span::styled("CHANNELS  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!(
                "{} up · {} pend · {} down",
                crate::format::commas(active),
                crate::format::commas(pending),
                crate::format::commas(inactive)
            ),
            Style::default().fg(Color::White),
        ),
    ])];
    if !streamer_mode && capacity > 0 {
        lines.push(Line::from(vec![
            Span::styled("BALANCE   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "{} / {} sats",
                    crate::format::commas(local),
                    crate::format::commas(capacity)
                ),
                Style::default().fg(Color::White),
            ),
        ]));
    }
    lines
}

fn get_price_ticker_footer(config: &AppConfig, state: &crate::app::AppState) -> (String, Style) {
    let (change, style) = get_price_variation_text(config, state);
    if state.price.last_error.is_some() {
        return (change, style);
    }

    let values: Vec<f64> = state
        .price_history
        .iter()
        .map(|(_, price)| *price)
        .collect();
    if values.is_empty() {
        return (change, style);
    }
    let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let low = values.iter().copied().fold(f64::INFINITY, f64::min);
    (
        format!(
            "{change}  ·  H {}  L {}",
            crate::format::commas(high.trunc() as u64),
            crate::format::commas(low.trunc() as u64)
        ),
        style,
    )
}

fn get_price_variation_text(config: &AppConfig, state: &crate::app::AppState) -> (String, Style) {
    if let Some(error) = state.price.last_error.as_deref() {
        let prefix = if state.price.last_price_in_currency.is_some() {
            "STALE"
        } else {
            "ERR"
        };
        let color = if state.price.last_price_in_currency.is_some() {
            Color::Yellow
        } else {
            Color::Red
        };
        return (format!("{prefix} · {error}"), Style::default().fg(color));
    }

    let style = get_price_style(config, state);
    match get_price_variation(config, state) {
        Some((change_pct, label)) => (format!("{:+.2}% / {}", change_pct, label), style),
        None => (
            "Change --".to_string(),
            Style::default().fg(Color::DarkGray),
        ),
    }
}

fn get_price_variation(config: &AppConfig, state: &crate::app::AppState) -> Option<(f64, String)> {
    let current = state.price.last_price_in_currency?;

    let (window, label) = price_variation_window(config);
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

    let reference = reference?;
    if reference == 0.0 {
        return None;
    }

    let change_pct = (current - reference) / reference * 100.0;
    Some((change_pct, label))
}

fn price_variation_window(config: &AppConfig) -> (std::time::Duration, String) {
    match config.price.variation.as_str() {
        "minute" => (std::time::Duration::from_secs(60), "1m".to_string()),
        "hour" => (std::time::Duration::from_secs(60 * 60), "1h".to_string()),
        "day" => (
            std::time::Duration::from_secs(60 * 60 * 24),
            "24h".to_string(),
        ),
        other => (std::time::Duration::from_secs(60), other.to_string()),
    }
}

fn get_price_block_style(state: &crate::app::AppState) -> Style {
    match (
        state.price.last_price_in_currency,
        state.price.last_error.as_deref(),
    ) {
        (Some(_), None) => Style::default().fg(Color::Green),
        (Some(_), Some(_)) => Style::default().fg(Color::Yellow),
        (None, _) => Style::default().fg(Color::Red),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppThread;
    use crate::config::{
        BitcoinCoreSettings, CoreLightningSettings, FeesSettings, LndSettings, NodeConfig,
        PriceSettings,
    };
    use crate::node::NodeState;
    use crate::widget::{DefaultWidgetState, DynamicNodeStatefulWidget, DynamicState};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::widgets::Paragraph;
    use ratatui::Terminal;
    use tokio::sync::mpsc;

    struct TestNodeWidget;

    impl DynamicNodeStatefulWidget for TestNodeWidget {
        fn render(
            &self,
            area: Rect,
            buf: &mut Buffer,
            _node_state: &mut NodeState,
            _config: &AppConfig,
        ) {
            Paragraph::new("Node detail").render(area, buf);
        }
    }

    fn test_config(node_count: usize, price: bool, fees: bool) -> AppConfig {
        AppConfig {
            tick_rate: "250".to_string(),
            streamer_mode: false,
            node_switch_interval: "5".to_string(),
            price: PriceSettings {
                enabled: price,
                currency: "USD".to_string(),
                big_text: true,
                variation: "minute".to_string(),
                variation_threshold: 0.0,
            },
            fees: FeesSettings { enabled: fees },
            bitcoin_core: BitcoinCoreSettings::default(),
            core_lightning: CoreLightningSettings::default(),
            lnd: LndSettings::default(),
            nodes: (0..node_count)
                .map(|index| NodeConfig {
                    name: Some(format!("Pi {}", index + 1)),
                    provider: "bitcoin_core".to_string(),
                    bitcoin_core: None,
                    core_lightning: None,
                    lnd: None,
                })
                .collect(),
        }
    }

    fn test_app(node_count: usize, price: bool, fees: bool) -> App {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let widgets = (0..node_count)
            .map(|_| Box::new(TestNodeWidget) as Box<dyn DynamicNodeStatefulWidget>)
            .collect();
        let states = (0..node_count)
            .map(|_| Box::new(DefaultWidgetState) as Box<dyn DynamicState>)
            .collect();
        let node_names = (0..node_count)
            .map(|index| format!("Pi {}", index + 1))
            .collect();
        App::new(
            AppThread::new(sender),
            widgets,
            states,
            node_names,
            test_config(node_count, price, fees),
        )
    }

    fn draw_at(mut app: App, width: u16, height: u16) -> App {
        let config = app.config.clone();
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(&config, &mut app, frame))
            .unwrap();
        app
    }

    #[test]
    fn multi_node_touch_layout_stays_inside_small_viewports() {
        for (width, height) in [(40, 12), (60, 20), (80, 24)] {
            let mut app = test_app(3, true, true);
            app.view_menu_open = true;
            let app = draw_at(app, width, height);

            assert_eq!(
                app.touch_targets
                    .iter()
                    .filter(|target| matches!(target.action, TouchAction::SelectView(_)))
                    .count(),
                4
            );
            for target in app.touch_targets {
                assert!(target.area.x.saturating_add(target.area.width) <= width);
                assert!(target.area.y.saturating_add(target.area.height) <= height);
            }
        }
    }

    #[test]
    fn price_only_keeps_the_entire_screen_when_there_is_no_second_view() {
        let app = draw_at(test_app(0, true, false), 60, 20);

        assert_eq!(app.active_view, DashboardView::Price);
        assert!(app.touch_targets.is_empty());
    }

    #[test]
    fn price_only_can_offer_fees_through_the_view_picker() {
        let mut app = test_app(0, true, true);
        app.view_menu_open = true;
        let mut app = draw_at(app, 60, 20);

        assert_eq!(
            app.touch_targets
                .iter()
                .filter(|target| matches!(target.action, TouchAction::SelectView(_)))
                .count(),
            2
        );
        assert!(app
            .touch_targets
            .iter()
            .any(|target| { target.action == TouchAction::SelectView(DashboardView::Fees) }));

        let fees_target = app
            .touch_targets
            .iter()
            .find(|target| target.action == TouchAction::SelectView(DashboardView::Fees))
            .copied()
            .unwrap();
        app.handle_mouse_events(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: fees_target.area.x + fees_target.area.width / 2,
            row: fees_target.area.y + fees_target.area.height / 2,
            modifiers: KeyModifiers::NONE,
        })
        .unwrap();

        assert_eq!(app.active_view, DashboardView::Fees);
        assert!(!app.view_menu_open);
    }

    #[test]
    fn node_only_dock_uses_three_live_controls() {
        let app = draw_at(test_app(3, false, false), 60, 20);

        assert_eq!(app.touch_targets.len(), 3);
        assert!(!app
            .touch_targets
            .iter()
            .any(|target| target.action == TouchAction::ToggleViewMenu));
    }

    #[test]
    fn touch_dock_is_hidden_when_the_terminal_is_too_short() {
        let app = draw_at(test_app(3, true, true), 40, 10);

        assert!(app.touch_targets.is_empty());
    }
}
