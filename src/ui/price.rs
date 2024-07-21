use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::{Block, BorderType, Padding, Paragraph},
};
use tui_big_text::{BigText, PixelSize};

use crate::price::PriceState;

use super::Draw;

impl Draw for PriceState {
    fn draw(&self, frame: &mut ratatui::Frame, area: Rect, style: Option<Style>) {
        let style = style.unwrap_or(Style::new());

        let lines = vec![match self.last_price_in_currency {
            Some(v) => vec![v.trunc().to_string(), self.currency.to_string()]
                .join(" ")
                .into(),
            None => "...".into(),
        }];

        let price_block = Block::bordered()
            .padding(Padding::top(1))
            .title("Price")
            .title_alignment(Alignment::Center)
            .border_type(BorderType::Plain)
            .style(style);

        let price_block_area = price_block.inner(area);
        frame.render_widget(price_block, area);

        if frame.size().width > 70 {
            frame.render_widget(
                BigText::builder()
                    .alignment(Alignment::Center)
                    .pixel_size(PixelSize::Sextant)
                    .style(style)
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
}
