use crate::{app::AppState, config::AppConfig, node::NodeStatus};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::Block,
    Frame,
};

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

    // let fee_state = bitcoin_state.fees.clone();
    // // fee_state.dedup_by(|a, b| a.fee == b.fee);

    // let fees: Vec<Line> = fee_state
    //     .iter()
    //     .map(|estimation| {
    //         let fee: f64 = estimation.fee.to_float_in(bitcoin::Denomination::SAT);
    //         Line::from(vec![
    //             Span::raw(estimation.received_target.to_string()),
    //             Span::raw(" blocks: "),
    //             Span::styled(
    //                 ((4.0 / 3000.0) * fee).trunc().to_string(),
    //                 Style::new().white().italic(),
    //             ),
    //             Span::styled(" sats/vbyte ", Style::new().white().italic()),
    //         ])
    //     })
    //     .collect();

    // let fees_block = Paragraph::new(fees)
    //     .block(
    //         Block::bordered()
    //             .padding(Padding::left(1))
    //             .title("Fees")
    //             .title_alignment(Alignment::Center)
    //             .border_type(BorderType::Plain),
    //     )
    //     .style(status_style);

    if config.price.enabled {
        state
            .price
            .draw(frame, *bottom_panel_right, Some(status_style));
        frame.render_widget(Block::new(), *bottom_panel_left);
    } else {
        // frame.render_widget(fees_block, main_layout[1]);
        state.price.draw(frame, *bottom_panel, Some(status_style));
    }

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
