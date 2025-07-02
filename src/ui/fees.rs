use ratatui::{
    layout::{Alignment, Rect},
    prelude::Stylize,
    style::Style,
    text::Line,
    text::Span,
    widgets::{Block, BorderType, Padding, Paragraph},
};

use crate::fees::FeesState;

use super::Draw;

impl Draw for FeesState {
    fn draw(&self, frame: &mut ratatui::Frame, area: Rect, style: Option<Style>) {
        let style = style.unwrap_or(Style::new());

        let fee_state = self.result.clone();
        // fee_state.dedup_by(|a, b| a.fee == b.fee);

        let fees: Vec<Option<Line>> = vec![
            Some(Line::from(Span::raw("Priority"))),
            get_fee_line("Low", fee_state.low),
            get_fee_line("Normal", fee_state.medium),
            get_fee_line("High", fee_state.high),
        ];
        
        let filtered_fees: Vec<Line> = fees.into_iter().filter_map(|opt| opt).collect();

        let fees_block = Paragraph::new(filtered_fees)
            .block(
                Block::bordered()
                    .padding(Padding::left(1))
                    .title("Fees")
                    .title_alignment(Alignment::Center)
                    .border_type(BorderType::Plain),
            )
            .style(style);

        frame.render_widget(fees_block, area);
    }
}

fn get_fee_line<'a>(name: &'a str, value: Option<String>) -> Option<Line<'a>> {
    if let Some(res) = value {
        return Some(Line::from(vec![
            Span::raw(name),
            Span::raw(": "),
            Span::styled(res, Style::new().white()),
            Span::styled(" Sats/vbyte ", Style::new().white()),
        ]));
    }

    None
}
