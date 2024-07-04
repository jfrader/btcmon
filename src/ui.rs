use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Padding, Paragraph},
    Frame,
};

use crate::app::App;

/// Renders the user interface widgets.
pub fn render(app: &mut App, frame: &mut Frame) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Length(frame.size().height - 1),
            Constraint::Max(1),
        ])
        .split(frame.size());

    let bitcoin_state_locked = app.bitcoin_state.clone();
    let bitcoin_state = bitcoin_state_locked.lock().unwrap();

    let text = vec![
        Line::from(vec![
            Span::raw("Block Height: "),
            Span::styled(
                bitcoin_state.current_height.to_string(),
                Style::new().green().italic(),
            ),
        ]),
        Line::from(vec![
            Span::raw("Latest Block: "),
            Span::styled(
                bitcoin_state.last_hash.clone(),
                Style::new().green().italic(),
            ),
        ]),
        "---".into(),
    ];

    frame.render_widget(
        Paragraph::new(text)
            .block(
                Block::bordered()
                    .padding(Padding::left(1))
                    .title("Bitcoin")
                    .title_alignment(Alignment::Center)
                    .border_type(BorderType::Plain),
            )
            .style(Style::default().fg(Color::Cyan).bg(Color::Black)),
        layout[0],
    );

    frame.render_widget(
        Paragraph::new(format!("Node {}", bitcoin_state.status))
            .block(Block::new().padding(Padding::left(1)))
            .style(Style::default().fg(Color::Cyan).bg(Color::Black)),
        layout[1],
    );
}
