use crate::app::App;
use crate::ui::components::centered_rect;
use crate::ui::theme::*;
use ratatui::{
    Frame,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

pub fn draw(f: &mut Frame, app: &App) {
    let area = centered_rect(80, 80, f.area());
    f.render_widget(Clear, area);

    let Some(session) = &app.shell_session else {
        return;
    };

    let screen = session.parser.screen();
    let (rows, cols) = screen.size();
    let cursor = screen.cursor_position();

    let inner_height = area.height.saturating_sub(2); // borders
    let inner_width = area.width.saturating_sub(2);

    let mut lines: Vec<Line> = Vec::with_capacity(rows as usize);

    for row in 0..rows.min(inner_height) {
        let mut spans: Vec<Span> = Vec::new();
        let mut col = 0u16;
        while col < cols.min(inner_width) {
            let cell = screen.cell(row, col);
            let (content, style) = match cell {
                Some(cell) => {
                    let mut s = Style::default();
                    s = s.fg(convert_color(cell.fgcolor()));
                    s = s.bg(convert_color(cell.bgcolor()));
                    if cell.bold() {
                        s = s.add_modifier(Modifier::BOLD);
                    }
                    if cell.underline() {
                        s = s.add_modifier(Modifier::UNDERLINED);
                    }
                    if cell.inverse() {
                        s = s.add_modifier(Modifier::REVERSED);
                    }
                    let text = cell.contents();
                    if text.is_empty() {
                        (" ".to_owned(), s)
                    } else {
                        (text.to_owned(), s)
                    }
                }
                None => (" ".to_owned(), Style::default()),
            };
            let style = if row == cursor.0 && col == cursor.1 {
                style.add_modifier(Modifier::REVERSED)
            } else {
                style
            };
            spans.push(Span::styled(content, style));
            col += 1;
        }
        lines.push(Line::from(spans));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Shell (Ctrl+Q to close)")
        .style(STYLE_NORMAL);

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn convert_color(c: vt100::Color) -> ratatui::style::Color {
    match c {
        vt100::Color::Default => ratatui::style::Color::Reset,
        vt100::Color::Idx(i) => ratatui::style::Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => ratatui::style::Color::Rgb(r, g, b),
    }
}
