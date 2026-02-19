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

    let lines: Vec<Line> = app.describe_content.iter().map(Line::raw).collect();

    let total_lines = lines.len() as u16;
    let visible_height = area.height.saturating_sub(2);

    let scroll = (app.describe_scroll as u16).min(total_lines.saturating_sub(visible_height));

    let title = format!("Describe [{} lines]", app.describe_content.len(),);

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(STYLE_NORMAL),
        )
        .style(STYLE_NORMAL)
        .scroll((scroll, 0));

    f.render_widget(paragraph, area);
}
