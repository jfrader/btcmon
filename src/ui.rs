use crate::{
    app::AppState,
    config::AppConfig,
    node::node::{NodeState, NodeStatus},
    price::price::PriceState,
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Padding, Paragraph},
    Frame,
};
use tui_big_text::{BigText, PixelSize};
use tui_popup::{Popup, SizedWrapper};

pub fn render(config: &AppConfig, state: &AppState, frame: &mut Frame) {
    let bitcoin_state = state.node.clone().unwrap().lock().unwrap().clone();

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Length(frame.size().height / 2),
            Constraint::Length(frame.size().height / 2 - 1),
            Constraint::Max(1),
        ])
        .split(frame.size());

    let second_pane_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(main_layout[1]);

    let status_style = get_status_style(&bitcoin_state.status);

    render_bitcoin(config, frame, &bitcoin_state, status_style, main_layout[0]);

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
        render_price(state.price, frame, status_style, second_pane_layout[1]);
        // frame.render_widget(fees_block, second_pane_layout[0]);
        frame.render_widget(Block::new(), second_pane_layout[0]);
    } else {
        // frame.render_widget(fees_block, main_layout[1]);
        frame.render_widget(Block::new(), main_layout[1]);
    }

    render_status_bar(frame, &bitcoin_state, main_layout[2]);
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

fn render_bitcoin(
    config: &AppConfig,
    frame: &mut Frame,
    bitcoin_state: &NodeState,
    status_style: Style,
    area: Rect,
) {
    let block_height = match bitcoin_state.status {
        NodeStatus::Synchronizing => Line::from(vec![
            Span::raw("Block Height: "),
            Span::styled(
                bitcoin_state.height.to_string(),
                Style::new().fg(Color::White).italic(),
            ),
            Span::raw("/"),
            Span::styled(
                bitcoin_state.headers.to_string(),
                Style::new().fg(Color::Blue).italic(),
            ),
        ]),
        _ => Line::from(vec![
            Span::raw("Block Height: "),
            Span::styled(
                bitcoin_state.height.to_string(),
                Style::new().fg(Color::White).italic(),
            ),
        ]),
    };

    let text: Vec<Line> = vec![
        block_height,
        Line::from(vec![
            Span::raw("Last Block: "),
            Span::styled(
                bitcoin_state.last_hash.clone(),
                Style::new().fg(Color::White).italic(),
            ),
        ]),
        "------".into(),
    ];

    frame.render_widget(
        Paragraph::new(text)
            .block(
                Block::bordered()
                    .padding(Padding::left(1))
                    .title(match config.bitcoin_core.host.as_str() {
                        "localhost" => "Bitcoin Core",
                        _ => config.bitcoin_core.host.as_str(),
                    })
                    .title_alignment(Alignment::Center)
                    .border_type(BorderType::Plain),
            )
            .style(status_style),
        area,
    );
}

fn render_price(state: PriceState, frame: &mut Frame, status_style: Style, area: Rect) {
    let lines = vec![match state.last_price_in_currency {
        Some(v) => vec![v.trunc().to_string(), state.currency.to_string()]
            .join(" ")
            .into(),
        None => "-".into(),
    }];

    let price_block = Block::bordered()
        .padding(Padding::top(1))
        .title("Price")
        .title_alignment(Alignment::Center)
        .border_type(BorderType::Plain)
        .style(status_style);

    let price_block_area = price_block.inner(area);
    frame.render_widget(price_block, area);

    if frame.size().width > 65 {
        frame.render_widget(
            BigText::builder()
                .alignment(Alignment::Center)
                .pixel_size(PixelSize::Quadrant)
                .style(status_style)
                .lines(lines)
                .build()
                .unwrap(),
            price_block_area,
        );
    } else {
        frame.render_widget(
            Paragraph::new(lines)
                .style(Style::default().fg(Color::White))
                .alignment(Alignment::Center),
            price_block_area,
        );
    }
}

fn get_status_style(status: &NodeStatus) -> Style {
    match status {
        NodeStatus::Online => Style::default().fg(Color::Green).bg(Color::Black),
        NodeStatus::Offline => Style::default().fg(Color::Red).bg(Color::Black),
        NodeStatus::Synchronizing => Style::default().fg(Color::Blue).bg(Color::Black),
    }
}
