use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Gauge, Padding, Paragraph, Widget};

use crate::node::NodeStatus;
use crate::ui::get_status_style;

pub struct BalanceGauge {
    local_balance: u64,
    total_capacity: u64,
}

impl BalanceGauge {
    pub fn new(local_balance: u64, total_capacity: u64) -> Self {
        Self {
            local_balance,
            total_capacity,
        }
    }
}

impl Widget for BalanceGauge {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let gauge = Gauge::default()
            .gauge_style(Style::new().fg(Color::Green).bg(Color::Black))
            .label(Span::styled(
                format!(
                    " local {} sats / remote {} sats ",
                    self.local_balance,
                    self.total_capacity - self.local_balance
                ),
                Style::new().fg(Color::White).bg(Color::Black),
            ))
            .ratio(if self.total_capacity > 0 {
                self.local_balance as f64 / self.total_capacity as f64
            } else {
                0.0
            });
        gauge.render(area, buf);
    }
}

/// A widget that displays a paragraph with lines inside a styled block.
pub struct BlockedParagraph<'a> {
    title: &'a str,
    status: NodeStatus,
    lines: Vec<Line<'a>>,
}

impl<'a> BlockedParagraph<'a> {
    pub fn new(title: &'a str, status: NodeStatus, lines: Vec<Line<'a>>) -> Self {
        Self {
            title,
            status,
            lines,
        }
    }
}

impl<'a> Widget for BlockedParagraph<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let style = get_status_style(&self.status);
        let block = Block::bordered()
            .padding(Padding::left(1))
            .title(self.title)
            .title_alignment(ratatui::layout::Alignment::Center)
            .border_type(BorderType::Plain)
            .style(style);

        let inner_area = block.inner(area);

        let paragraph = Paragraph::new(self.lines);

        block.render(area, buf);
        paragraph.render(inner_area, buf);
    }
}

/// A widget that displays a paragraph with lines and a gauge inside a styled block.
pub struct BlockedParagraphWithGauge<'a> {
    title: &'a str,
    status: NodeStatus,
    lines: Vec<Line<'a>>,
    local_balance: u64,
    total_capacity: u64,
}

impl<'a> BlockedParagraphWithGauge<'a> {
    pub fn new(
        title: &'a str,
        status: NodeStatus,
        lines: Vec<Line<'a>>,
        local_balance: u64,
        total_capacity: u64,
    ) -> Self {
        Self {
            title,
            status,
            lines,
            local_balance,
            total_capacity,
        }
    }
}

impl<'a> Widget for BlockedParagraphWithGauge<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let style = get_status_style(&self.status);
        let block = Block::bordered()
            .padding(Padding::left(1))
            .title(self.title)
            .title_alignment(ratatui::layout::Alignment::Center)
            .border_type(BorderType::Plain)
            .style(style);

        let inner_area = block.inner(area);
        block.render(area, buf);

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(self.lines.len() as u16),
                Constraint::Length(3),
            ])
            .split(inner_area);

        let paragraph = Paragraph::new(self.lines);
        paragraph.render(layout[0], buf);

        let gauge = BalanceGauge::new(self.local_balance, self.total_capacity);
        gauge.render(layout[1], buf);
    }
}
