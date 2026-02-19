use crate::app::App;
use crate::ui::theme::*;
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let lines: Vec<Line> = app.log_buffer.iter().map(Line::raw).collect();

    let total_lines = lines.len() as u16;
    let visible_height = area.height.saturating_sub(2);

    let (scroll, mode_label) = match app.log_scroll_offset {
        None => (total_lines.saturating_sub(visible_height), "FOLLOWING"),
        Some(offset) => {
            let offset = u16::try_from(offset).unwrap_or(u16::MAX);
            (
                offset.min(total_lines.saturating_sub(visible_height)),
                "PAUSED",
            )
        }
    };

    let title = format!("Logs [{} lines] [{}]", app.log_buffer.len(), mode_label,);

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .style(STYLE_NORMAL)
        .scroll((scroll, 0));

    f.render_widget(paragraph, area);
}
