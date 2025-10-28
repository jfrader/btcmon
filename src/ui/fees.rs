use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding, Paragraph, StatefulWidget, Widget};

use crate::app::AppState;

pub struct FeesWidget {
    pub style: Style,
}

impl StatefulWidget for FeesWidget {
    type State = AppState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let fee_state = state.fees.result.clone();

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
            .style(self.style);

        fees_block.render(area, buf);
    }
}

fn get_fee_line<'a>(name: &'a str, value: Option<String>) -> Option<Line<'a>> {
    if let Some(res) = value {
        return Some(Line::from(vec![
            Span::raw(name),
            Span::raw(": "),
            Span::styled(res, Style::new().fg(Color::White)),
            Span::styled(" Sats/vbyte ", Style::new().fg(Color::White)),
        ]));
    }
    None
}