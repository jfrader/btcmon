use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, BorderType, Padding, Paragraph, StatefulWidget, Widget};
use tui_big_text::{BigText, PixelSize};

use crate::app::AppState;
use crate::ui::get_status_style;

pub struct PriceWidget;

impl StatefulWidget for PriceWidget {
    type State = AppState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let style = get_status_style(&state.node.status);

        let price_with_currency_lines = vec![match state.price.last_price_in_currency {
            Some(v) => vec![v.trunc().to_string(), state.price.currency.to_string()]
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
        price_block.render(area, buf);

        if area.width > 48 {
            let big_text = BigText::builder()
                .alignment(Alignment::Center)
                .pixel_size(PixelSize::Sextant)
                .style(style)
                .lines(price_with_currency_lines)
                .build()
                .unwrap();
            big_text.render(price_block_area, buf);
        } else if area.width > 24 {

            let price_lines = match state.price.last_price_in_currency {
                Some(v) => vec![v.trunc().to_string().into(), state.price.currency.to_string().into()],
                None => vec!["...".into()],
            };

            let big_text = BigText::builder()
                .alignment(Alignment::Center)
                .pixel_size(PixelSize::Sextant)
                .style(style)
                .lines(price_lines)
                .build()
                .unwrap();
            big_text.render(price_block_area, buf);
        } else {
            Paragraph::new(price_with_currency_lines)
                .style(Style::default().fg(Color::White))
                .alignment(Alignment::Center)
                .render(price_block_area, buf);
        }
    }
}
