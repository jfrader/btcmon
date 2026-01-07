use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, BorderType, Paragraph, StatefulWidget, Widget};
use tui_widgets::big_text::{BigText, PixelSize};

use crate::app::AppState;

#[derive(Clone, Debug)]
pub struct PriceWidgetOptions {
    pub big_text: bool,
    pub style: Style,
    pub pixel_size: PixelSize,
    pub price_style: Style,
    pub title: String,
}

impl Default for PriceWidgetOptions {
    fn default() -> Self {
        PriceWidgetOptions {
            big_text: true,
            style: Style::default(),
            pixel_size: PixelSize::Sextant,
            price_style: Style::default(),
            title: "Price".to_string(),
        }
    }
}

pub struct PriceWidget {
    options: PriceWidgetOptions,
}

impl PriceWidget {
    pub fn new(options: PriceWidgetOptions) -> Self {
        PriceWidget { options }
    }
}

impl StatefulWidget for PriceWidget {
    type State = AppState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let price_with_currency_lines = vec![match state.price.last_price_in_currency {
            Some(v) => vec![v.trunc().to_string(), state.price.currency.to_string()]
                .join(" ")
                .into(),
            None => "...".into(),
        }];

        let price_block = Block::bordered()
            .title(self.options.title)
            .title_alignment(Alignment::Center)
            .border_type(BorderType::Plain)
            .style(self.options.style);

        let price_block_area = price_block.inner(area);
        price_block.render(area, buf);

        if self.options.big_text {
            if area.width > 48 {
                let content_area = centered_area(
                    price_block_area,
                    big_text_height(price_with_currency_lines.len(), self.options.pixel_size),
                );
                let big_text = BigText::builder()
                    .alignment(Alignment::Center)
                    .pixel_size(self.options.pixel_size)
                    .style(self.options.price_style)
                    .lines(price_with_currency_lines)
                    .build();

                big_text.render(content_area, buf);

                return;
            } else if area.width > 24 {
                let price_lines = match state.price.last_price_in_currency {
                    Some(v) => vec![
                        v.trunc().to_string().into(),
                        state.price.currency.to_string().into(),
                    ],
                    None => vec!["...".into()],
                };

                let content_area = centered_area(
                    price_block_area,
                    big_text_height(price_lines.len(), self.options.pixel_size),
                );
                let big_text = BigText::builder()
                    .alignment(Alignment::Center)
                    .pixel_size(self.options.pixel_size)
                    .style(self.options.price_style)
                    .lines(price_lines)
                    .build();

                big_text.render(content_area, buf);

                return;
            }
        }

        let content_area = centered_area(price_block_area, price_with_currency_lines.len() as u16);
        Paragraph::new(price_with_currency_lines)
            .style(self.options.price_style)
            .alignment(Alignment::Center)
            .render(content_area, buf);
    }
}

fn centered_area(area: Rect, content_height: u16) -> Rect {
    if content_height == 0 || area.height == 0 {
        return area;
    }

    let content_height = content_height.min(area.height);
    let offset = area.height.saturating_sub(content_height) / 2;
    Rect::new(area.x, area.y + offset, area.width, content_height)
}

fn big_text_height(line_count: usize, pixel_size: PixelSize) -> u16 {
    let line_height: u16 = match pixel_size {
        PixelSize::Full => 8,
        PixelSize::HalfHeight => 4,
        PixelSize::HalfWidth => 8,
        PixelSize::Quadrant => 4,
        PixelSize::ThirdHeight => 3,
        PixelSize::Sextant => 3,
    };
    line_height.saturating_mul(line_count as u16)
}
