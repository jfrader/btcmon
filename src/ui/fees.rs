use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, StatefulWidget, Widget};
use tui_widgets::big_text::{BigText, PixelSize};

use crate::app::AppState;

pub struct FeesWidget {
    pub style: Style,
}

impl StatefulWidget for FeesWidget {
    type State = AppState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let entries: Vec<(&str, String)> = [
            ("LOW", state.fees.result.low.clone()),
            ("NORMAL", state.fees.result.medium.clone()),
            ("FAST", state.fees.result.high.clone()),
        ]
        .into_iter()
        .filter_map(|(label, value)| value.map(|value| (label, value)))
        .collect();

        let title = if state.fees.last_error.is_some() && !entries.is_empty() {
            "Fees · sat/vB · STALE"
        } else {
            "Fees · sat/vB"
        };
        let block = Block::bordered()
            .title(title)
            .title_alignment(Alignment::Center)
            .border_type(BorderType::Plain)
            .border_style(self.style);
        let inner = block.inner(area);
        block.render(area, buf);

        if entries.is_empty() {
            let (message, style) = match state.fees.last_error.as_deref() {
                Some(error) => (format!("ERR · {error}"), Style::default().fg(Color::Red)),
                None => (
                    "Waiting for fee data...".to_string(),
                    Style::default().fg(Color::DarkGray),
                ),
            };
            Paragraph::new(message)
                .style(style)
                .alignment(Alignment::Center)
                .render(centered_area(inner, 1), buf);
            return;
        }

        if can_render_expanded(inner, &entries) {
            render_expanded(inner, buf, &entries);
        } else {
            render_compact(inner, buf, &entries);
        }
    }
}

fn can_render_expanded(area: Rect, entries: &[(&str, String)]) -> bool {
    if entries.is_empty() {
        return false;
    }
    let column_width = area.width / entries.len() as u16;
    let widest_value = entries
        .iter()
        .map(|(_, value)| value.chars().count() as u16 * 4)
        .max()
        .unwrap_or(0);
    area.height >= 7 && column_width >= widest_value.max(10)
}

fn render_expanded(area: Rect, buf: &mut Buffer, entries: &[(&str, String)]) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints((0..entries.len()).map(|_| Constraint::Ratio(1, entries.len() as u32)))
        .split(area);

    for (index, ((label, value), column)) in entries.iter().zip(columns.iter()).enumerate() {
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(4),
                Constraint::Length(1),
            ])
            .split(*column);

        Paragraph::new(*label)
            .style(
                Style::default()
                    .fg(Color::Rgb(247, 147, 26))
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .render(sections[0], buf);

        let value_area = centered_area(sections[1], 4);
        let big_value = BigText::builder()
            .pixel_size(PixelSize::Quadrant)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::White))
            .lines(vec![value.as_str().into()])
            .build();
        big_value.render(value_area, buf);

        Paragraph::new("sat/vB")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .render(sections[2], buf);

        if index + 1 < entries.len() {
            Block::new()
                .borders(Borders::RIGHT)
                .border_style(Style::default().fg(Color::DarkGray))
                .render(*column, buf);
        }
    }
}

fn render_compact(area: Rect, buf: &mut Buffer, entries: &[(&str, String)]) {
    let lines: Vec<Line> = entries
        .iter()
        .map(|(label, value)| {
            Line::from(vec![
                Span::styled(
                    format!("{label:<7}"),
                    Style::default().fg(Color::Rgb(247, 147, 26)),
                ),
                Span::styled(value, Style::default().fg(Color::White)),
            ])
        })
        .collect();
    let line_count = lines.len() as u16;
    Paragraph::new(lines).render(centered_area(area, line_count), buf);
}

fn centered_area(area: Rect, height: u16) -> Rect {
    let height = height.min(area.height);
    Rect::new(
        area.x,
        area.y + area.height.saturating_sub(height) / 2,
        area.width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_digit_fees_fall_back_when_columns_are_too_narrow() {
        let entries = vec![("NORMAL", "100".to_string()), ("FAST", "120".to_string())];

        assert!(!can_render_expanded(Rect::new(0, 0, 23, 7), &entries));
        assert!(can_render_expanded(Rect::new(0, 0, 24, 7), &entries));
    }
}
