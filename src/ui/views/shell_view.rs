use crate::app::App;
use crate::ui::components::centered_rect;
use crate::ui::theme::*;
use ratatui::{
    Frame,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

fn cursor_glyph_position(screen: &vt100::Screen) -> (u16, u16) {
    let (row, col) = screen.cursor_position();
    if screen
        .cell(row, col)
        .is_some_and(vt100::Cell::is_wide_continuation)
    {
        (row, col.saturating_sub(1))
    } else {
        (row, col)
    }
}

pub(crate) fn screen_lines(
    screen: &vt100::Screen,
    max_rows: u16,
    max_cols: u16,
) -> Vec<Line<'static>> {
    let (rows, cols) = screen.size();
    let (cursor_row, cursor_col) = cursor_glyph_position(screen);
    let mut lines: Vec<Line> = Vec::with_capacity(rows.min(max_rows) as usize);

    for row in 0..rows.min(max_rows) {
        let mut spans: Vec<Span> = Vec::new();
        let mut current_style = Style::default();
        let mut space_count = 0;
        let mut text_buf = String::new();
        let mut first_cell = true;

        let cols_limit = cols.min(max_cols);
        for col in 0..cols_limit {
            let cell = screen.cell(row, col);
            if cell.is_some_and(vt100::Cell::is_wide_continuation) {
                continue;
            }
            let is_cursor = row == cursor_row && col == cursor_col;

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

            let txt_ref: &str = contents.unwrap_or(" ");
            let cell_is_space = txt_ref == " ";

            if is_cursor {
                style = style.add_modifier(Modifier::REVERSED);
            }

            if first_cell {
                current_style = style;
                first_cell = false;
            } else if style != current_style {
                if space_count > 0 {
                    spans.push(Span::styled(get_spaces(space_count), current_style));
                    space_count = 0;
                } else if !text_buf.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut text_buf), current_style));
                }
                current_style = style;
            }

            if text_buf.is_empty() && cell_is_space {
                space_count += 1;
            } else {
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

    lines
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = centered_rect(80, 80, f.area());
    f.render_widget(Clear, area);

    let Some(session) = &app.shell_session else {
        return;
    };

    let lines = screen_lines(
        session.parser.screen(),
        area.height.saturating_sub(2),
        area.width.saturating_sub(2),
    );

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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;

    fn buffer_of(input: &[u8], cols: u16, max_cols: u16) -> Buffer {
        let mut parser = vt100::Parser::new(2, cols, 0);
        parser.process(input);
        let lines = screen_lines(parser.screen(), 2, max_cols);
        let area = Rect::new(0, 0, cols, 2);
        let mut buf = Buffer::empty(area);
        Paragraph::new(lines).render(area, &mut buf);
        buf
    }

    fn symbols(buf: &Buffer, row: u16, cols: u16) -> Vec<&str> {
        (0..cols).map(|c| buf[(c, row)].symbol()).collect()
    }

    fn row_width(input: &[u8], cols: u16) -> usize {
        let mut parser = vt100::Parser::new(2, cols, 0);
        parser.process(input);
        screen_lines(parser.screen(), 2, cols)[0].width()
    }

    #[test]
    fn wide_characters_do_not_shift_the_rest_of_the_row() {
        let buf = buffer_of("日本x".as_bytes(), 10, 10);
        assert_eq!(symbols(&buf, 0, 6), ["日", " ", "本", " ", "x", " "]);
        assert_eq!(row_width("日本x".as_bytes(), 10), 10);
    }

    #[test]
    fn emoji_keeps_following_text_aligned() {
        let buf = buffer_of("🔥ok".as_bytes(), 10, 10);
        assert_eq!(symbols(&buf, 0, 5), ["🔥", " ", "o", "k", " "]);
    }

    #[test]
    fn ascii_row_is_unchanged() {
        let buf = buffer_of(b"hello", 10, 10);
        assert_eq!(symbols(&buf, 0, 6), ["h", "e", "l", "l", "o", " "]);
        assert_eq!(row_width(b"hello", 10), 10);
    }

    #[test]
    fn interior_spaces_are_preserved() {
        let buf = buffer_of(b"a  b", 10, 10);
        assert_eq!(symbols(&buf, 0, 4), ["a", " ", " ", "b"]);
    }

    #[test]
    fn cursor_on_a_continuation_cell_highlights_the_glyph() {
        let buf = buffer_of("日本\x1b[1D".as_bytes(), 10, 10);
        assert!(
            buf[(2, 0)].modifier.contains(Modifier::REVERSED),
            "the cursor must land on the leading half of the wide glyph"
        );
    }

    #[test]
    fn cursor_after_a_wide_glyph_lands_on_the_next_column() {
        let buf = buffer_of("日x".as_bytes(), 10, 10);
        assert!(buf[(3, 0)].modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn row_is_clipped_to_the_available_width() {
        let buf = buffer_of(b"abcdefgh", 10, 4);
        assert_eq!(symbols(&buf, 0, 6), ["a", "b", "c", "d", " ", " "]);
    }

    #[test]
    fn a_wide_glyph_straddling_the_clip_is_dropped_by_ratatui() {
        let buf = buffer_of("日本".as_bytes(), 3, 3);
        assert_eq!(symbols(&buf, 0, 3), ["日", " ", " "]);
    }
}
