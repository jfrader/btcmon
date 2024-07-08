use crate::{app::App, bitcoin::EBitcoinNodeStatus, config::Settings};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Padding, Paragraph},
    Frame,
};
use tui_big_text::{BigText, PixelSize};
use tui_popup::{Popup, SizedWrapper};

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

    let popup = Popup::new(" New block! ", sized_paragraph).style(Style::new().white().on_black());
    frame.render_widget(&popup, frame.size());
}

/// Renders the user interface widgets.
pub fn render(config: &Settings, app: &mut App, frame: &mut Frame) {
    let bitcoin_state_unlocked = app.bitcoin_state.clone();
    let bitcoin_state_locked = bitcoin_state_unlocked.lock().unwrap();
    let bitcoin_state = bitcoin_state_locked.clone();

    drop(bitcoin_state_locked);

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

    let status_bar_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![
            Constraint::Length(1),
            Constraint::Length(frame.size().width - 1),
        ])
        .split(main_layout[2]);

    let status_style = get_status_style(&bitcoin_state.status);

    let fee_state = bitcoin_state.fees.clone();
    // fee_state.dedup_by(|a, b| a.fee == b.fee);

    let fees: Vec<Line> = fee_state
        .iter()
        .map(|estimation| {
            let fee: f64 = estimation.fee.to_float_in(bitcoin::Denomination::SAT);
            Line::from(vec![
                Span::raw(estimation.received_target.to_string()),
                Span::raw(" blocks: "),
                Span::styled(
                    ((4.0 / 3000.0) * fee).trunc().to_string(),
                    Style::new().white().italic(),
                ),
                Span::styled(" sats/vbyte ", Style::new().white().italic()),
            ])
        })
        .collect();

    let block_height = match bitcoin_state.status {
        EBitcoinNodeStatus::Synchronizing => Line::from(vec![
            Span::raw("Block Height: "),
            Span::styled(
                bitcoin_state.current_height.to_string(),
                Style::new().white().italic(),
            ),
            Span::raw("/"),
            Span::styled(
                bitcoin_state.header_height.to_string(),
                Style::new().blue().italic(),
            ),
        ]),
        _ => Line::from(vec![
            Span::raw("Block Height: "),
            Span::styled(
                bitcoin_state.current_height.to_string(),
                Style::new().white().italic(),
            ),
        ]),
    };

    let text: Vec<Line> = vec![
        block_height,
        Line::from(vec![
            Span::raw("Last Block: "),
            Span::styled(
                bitcoin_state.last_hash.clone(),
                Style::new().white().italic(),
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
        main_layout[0],
    );

    let fees_block = Paragraph::new(fees)
        .block(
            Block::bordered()
                .padding(Padding::left(1))
                .title("Fees")
                .title_alignment(Alignment::Center)
                .border_type(BorderType::Plain),
        )
        .style(status_style);

    if config.price.enabled {
        frame.render_widget(fees_block, second_pane_layout[0]);

        let big_price = BigText::builder()
            .alignment(Alignment::Center)
            .pixel_size(PixelSize::Quadrant)
            .style(status_style)
            .lines(vec![match app.price_state.last_price_in_currency {
                Some(v) => vec![v.trunc().to_string(), app.price_state.currency.to_string()]
                    .join(" ")
                    .into(),
                None => "-".into(),
            }])
            .build()
            .unwrap();

        let price_block = Block::bordered()
            .padding(Padding::top(1))
            .title("Price")
            .title_alignment(Alignment::Center)
            .border_type(BorderType::Plain)
            .style(status_style);

        let price_block_area = price_block.inner(second_pane_layout[1]);
        frame.render_widget(price_block, second_pane_layout[1]);
        frame.render_widget(big_price, price_block_area);
    } else {
        frame.render_widget(fees_block, main_layout[1]);
    }

    if bitcoin_state.status == EBitcoinNodeStatus::Connecting
        || bitcoin_state.status == EBitcoinNodeStatus::Synchronizing
    {
        let throbber = throbber_widgets_tui::Throbber::default()
            .throbber_set(throbber_widgets_tui::QUADRANT_BLOCK_CRACK);
        frame.render_widget(throbber, status_bar_layout[0]);
    } else {
        frame.render_widget(
            Block::new().style(Style::default().fg(Color::White).bg(Color::Black)),
            status_bar_layout[0],
        );
    }

    frame.render_widget(
        Paragraph::new(format!("Node {}", bitcoin_state.status))
            .block(Block::new().padding(Padding::left(1)))
            .style(Style::default().fg(Color::White).bg(Color::Black)),
        status_bar_layout[1],
    );

    if let Some(time) = bitcoin_state.last_hash_time {
        if time.elapsed().as_secs() < 15 && bitcoin_state.status == EBitcoinNodeStatus::Online {
            render_newblock_popup(frame, bitcoin_state.current_height);
        }
    }
}

fn get_status_style(status: &EBitcoinNodeStatus) -> Style {
    match status {
        EBitcoinNodeStatus::Online => Style::default().fg(Color::Green).bg(Color::Black),
        EBitcoinNodeStatus::Offline => Style::default().fg(Color::Red).bg(Color::Black),
        EBitcoinNodeStatus::Connecting => Style::default().fg(Color::Blue).bg(Color::Black),
        EBitcoinNodeStatus::Synchronizing => Style::default().fg(Color::Blue).bg(Color::Black),
    }
}
