use crate::app::App;
use crate::ui::theme::*;
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let total_lines = app.log_buffer.len();
    let visible_height = area.height.saturating_sub(2) as usize;

    let (scroll_offset, mode_label) = match app.log_scroll_offset {
        None => (total_lines.saturating_sub(visible_height), "FOLLOWING"),
        Some(offset) => (
            offset.min(total_lines.saturating_sub(visible_height)),
            "PAUSED",
        ),
    };

    let end = (scroll_offset + visible_height).min(total_lines);
    let lines: Vec<Line> = (scroll_offset..end)
        .map(|i| Line::raw(&*app.log_buffer[i]))
        .collect();

    let history_label = if app.log_loading_history {
        " [Loading...]"
    } else {
        ""
    };
    let title = format!(
        "Logs [{} lines] [{}]{}",
        total_lines, mode_label, history_label,
    );

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .style(STYLE_NORMAL);

    f.render_widget(paragraph, area);
}
