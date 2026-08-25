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
        let price_with_currency_lines: Vec<ratatui::text::Line<'static>> =
            match state.price.last_price_in_currency {
                Some(v) => vec![
                    v.trunc().to_string().into(),
                    state.price.currency.to_string().into(),
                ],
                None => vec!["...".into()],
            };

        let price_block = Block::bordered()
            .title(self.options.title)
            .title_alignment(Alignment::Center)
            .border_type(BorderType::Plain)
            .style(self.options.style);

        let price_block_area = price_block.inner(area);
        price_block.render(area, buf);

        if self.options.big_text {
            let longest_line = price_with_currency_lines
                .iter()
                .map(|line| line.width())
                .max()
                .unwrap_or(0);
            if let Some(pixel_size) = fitting_pixel_size(
                self.options.pixel_size,
                price_block_area,
                price_with_currency_lines.len(),
                longest_line,
            ) {
                let content_area = centered_area(
                    price_block_area,
                    big_text_height(price_with_currency_lines.len(), pixel_size),
                );
                let big_text = BigText::builder()
                    .alignment(Alignment::Center)
                    .pixel_size(pixel_size)
                    .style(self.options.price_style)
                    .lines(price_with_currency_lines)
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
    let (_, line_height) = pixel_dimensions(pixel_size);
    line_height.saturating_mul(line_count as u16)
}

fn fitting_pixel_size(
    preferred: PixelSize,
    area: Rect,
    line_count: usize,
    longest_line: usize,
) -> Option<PixelSize> {
    let candidates: &[PixelSize] = match preferred {
        PixelSize::Full => &[
            PixelSize::Full,
            PixelSize::HalfHeight,
            PixelSize::HalfWidth,
            PixelSize::Quadrant,
            PixelSize::Sextant,
        ],
        PixelSize::HalfHeight => &[
            PixelSize::HalfHeight,
            PixelSize::Quadrant,
            PixelSize::Sextant,
        ],
        PixelSize::HalfWidth => &[
            PixelSize::HalfWidth,
            PixelSize::Quadrant,
            PixelSize::Sextant,
        ],
        PixelSize::Quadrant => &[PixelSize::Quadrant, PixelSize::Sextant],
        PixelSize::ThirdHeight => &[PixelSize::ThirdHeight, PixelSize::Sextant],
        PixelSize::Sextant => &[PixelSize::Sextant],
    };

    candidates.iter().copied().find(|pixel_size| {
        let (character_width, line_height) = pixel_dimensions(*pixel_size);
        let required_width = character_width.saturating_mul(longest_line as u16);
        let required_height = line_height.saturating_mul(line_count as u16);
        required_width <= area.width && required_height <= area.height
    })
}

fn pixel_dimensions(pixel_size: PixelSize) -> (u16, u16) {
    match pixel_size {
        PixelSize::Full => (8, 8),
        PixelSize::HalfHeight => (8, 4),
        PixelSize::HalfWidth => (4, 8),
        PixelSize::Quadrant => (4, 4),
        PixelSize::ThirdHeight => (8, 3),
        PixelSize::Sextant => (4, 3),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_price_text_falls_back_when_six_digits_do_not_fit() {
        let size = fitting_pixel_size(PixelSize::Full, Rect::new(0, 0, 42, 16), 2, 6);

        assert_eq!(size, Some(PixelSize::HalfWidth));
    }

    #[test]
    fn docked_price_uses_half_height_instead_of_small_quadrants() {
        let size = fitting_pixel_size(PixelSize::Full, Rect::new(0, 0, 58, 14), 2, 6);

        assert_eq!(size, Some(PixelSize::HalfHeight));
    }

    #[test]
    fn price_text_uses_plain_text_when_no_big_pixel_size_fits() {
        let size = fitting_pixel_size(PixelSize::Quadrant, Rect::new(0, 0, 20, 5), 2, 6);

        assert_eq!(size, None);
    }
}
