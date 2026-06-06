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
        let mut current_style = Style::default();
        let mut space_count = 0;
        let mut text_buf = String::new();

        let cols_limit = cols.min(inner_width);
        for col in 0..cols_limit {
            let cell = screen.cell(row, col);
            let is_cursor = row == cursor.0 && col == cursor.1;

            let (contents, mut style) = match cell {
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
                    let txt = cell.contents();
                    if txt.is_empty() {
                        (None, s)
                    } else {
                        (Some(txt), s)
                    }
                }
                None => (None, Style::default()),
            };

            let txt_ref = contents.unwrap_or(" ");
            let cell_is_space = txt_ref == " ";

            if is_cursor {
                style = style.add_modifier(Modifier::REVERSED);
            }

            let style_changed = col > 0 && style != current_style;

            if style_changed {
                if space_count > 0 {
                    spans.push(Span::styled(get_spaces(space_count), current_style));
                    space_count = 0;
                } else if !text_buf.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut text_buf), current_style));
                }
                current_style = style;
            }

            if text_buf.is_empty() && cell_is_space {
                if col == 0 {
                    current_style = style;
                }
                space_count += 1;
            } else {
                if col == 0 {
                    current_style = style;
                }
                if space_count > 0 {
                    text_buf.push_str(get_spaces(space_count));
                    space_count = 0;
                }
                text_buf.push_str(txt_ref);
            }
        }
        if space_count > 0 {
            spans.push(Span::styled(get_spaces(space_count), current_style));
        } else if !text_buf.is_empty() {
            spans.push(Span::styled(text_buf, current_style));
        }
        lines.push(Line::from(spans));
    }

    let title = if app.shell_title.is_empty() {
        "Shell (Ctrl+Q to close)".to_string()
    } else {
        format!("{} (Ctrl+Q to close)", app.shell_title)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
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

fn get_spaces(n: usize) -> &'static str {
    const SPACES: &str = "                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                ";
    &SPACES[..n.min(SPACES.len())]
}
