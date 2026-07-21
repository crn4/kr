use crate::app::App;
use crate::ui::components::centered_rect;
use crate::ui::theme::*;
use ratatui::{
    Frame,
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
};

pub fn draw(f: &mut Frame, app: &App) {
    let area = centered_rect(90, 90, f.area());
    f.render_widget(Clear, area);

    let total_lines = app.describe_content.len();
    let visible_height = area.height.saturating_sub(2) as usize;

    let scroll = app
        .describe_scroll
        .min(total_lines.saturating_sub(visible_height));
    let end = (scroll + visible_height).min(total_lines);

    let lines: Vec<Line> = app.describe_content[scroll..end]
        .iter()
        .map(|l| Line::raw(l.as_str()))
        .collect();

    let title = format!("Describe [{} lines]", total_lines);

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(STYLE_NORMAL),
        )
        .style(STYLE_NORMAL)
        .scroll((0, app.describe_hscroll as u16));

    f.render_widget(paragraph, area);
}
