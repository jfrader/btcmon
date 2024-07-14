use crate::{
    app::AppState,
    config::AppConfig,
    node::{NodeState, NodeStatus},
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph},
    Frame,
};
use tui_popup::{Popup, SizedWrapper};

pub mod node;
pub mod price;

pub trait Draw {
    fn draw(&self, frame: &mut Frame, area: Rect, style: Option<Style>);
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
        .constraints(vec![Constraint::Percentage(35), Constraint::Percentage(65)])
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

    render_status_bar(frame, &node, *status_panel);
}

fn render_newblock_popup(frame: &mut Frame, height: u64) {
    let sized_paragraph = SizedWrapper {
        inner: Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![Span::raw("Height")]),
            Line::from(vec![Span::raw(height.to_string())]),
            Line::from(""),
        ])
        .centered(),
        width: 21,
        height: 4,
    };

    let popup = Popup::new(" New block! ", sized_paragraph)
        .style(Style::new().fg(Color::White).bg(Color::Black));
    frame.render_widget(&popup, frame.size());
}

fn render_status_bar(frame: &mut Frame, bitcoin_state: &NodeState, area: Rect) {
    let zmq_status_width = 12;
    let status_bar_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![
            Constraint::Length(1),
            Constraint::Length(frame.size().width - zmq_status_width - 1),
            Constraint::Length(zmq_status_width),
        ])
        .split(area);

    if bitcoin_state.status == NodeStatus::Synchronizing {
        let throbber = throbber_widgets_tui::Throbber::default()
            .throbber_set(throbber_widgets_tui::QUADRANT_BLOCK_CRACK);
        frame.render_widget(throbber, status_bar_layout[0]);
    } else {
        frame.render_widget(
            Block::new().style(Style::default().fg(Color::White).bg(Color::Black)),
            status_bar_layout[0],
        );
    }

    // frame.render_widget(
    //     Paragraph::new(format!("ZMQ {} ", bitcoin_state.zmq_status))
    //         .style(Style::default().fg(Color::White).bg(Color::Black)),
    //     status_bar_layout[2],
    // );

    frame.render_widget(
        Paragraph::new(format!("Node {}", bitcoin_state.status))
            .block(Block::new().padding(Padding::left(1)))
            .style(Style::default().fg(Color::White).bg(Color::Black)),
        status_bar_layout[1],
    );

    if let Some(time) = bitcoin_state.last_hash_instant {
        if time.elapsed().as_secs() < 15 && bitcoin_state.status == NodeStatus::Online {
            render_newblock_popup(frame, bitcoin_state.height);
        }
    }
}

pub fn get_status_style(status: &NodeStatus) -> Style {
    match status {
        NodeStatus::Online => Style::default().fg(Color::Green).bg(Color::Black),
        NodeStatus::Offline => Style::default().fg(Color::Red).bg(Color::Black),
        NodeStatus::Synchronizing => Style::default().fg(Color::Blue).bg(Color::Black),
    }
}
