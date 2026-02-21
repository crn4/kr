use crate::app::App;
use crate::models::AppMode;
use crate::ui::theme::*;
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

fn highlight_line<'a>(text: &'a str, needle_lower: &str) -> Line<'a> {
    if needle_lower.is_empty() {
        return Line::raw(text);
    }
    let needle_len = needle_lower.len();
    let text_bytes = text.as_bytes();
    let needle_bytes = needle_lower.as_bytes();
    let mut spans = Vec::with_capacity(4);
    let mut start = 0;
    while start + needle_len <= text_bytes.len() {
        if let Some(pos) = text_bytes[start..]
            .windows(needle_len)
            .position(|w| w.eq_ignore_ascii_case(needle_bytes))
        {
            let abs = start + pos;
            if abs > start {
                spans.push(Span::raw(&text[start..abs]));
            }
            spans.push(Span::styled(
                &text[abs..abs + needle_len],
                STYLE_SEARCH_MATCH,
            ));
            start = abs + needle_len;
        } else {
            break;
        }
    }
    if start < text.len() {
        spans.push(Span::raw(&text[start..]));
    }
    if spans.is_empty() {
        Line::raw(text)
    } else {
        Line::from(spans)
    }
}

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

    let temp;
    let query_lower = if app.mode == AppMode::LogSearchInput {
        temp = app.log_search_input.to_ascii_lowercase();
        temp.as_str()
    } else {
        app.log_search_query.as_str()
    };

    let end = (scroll_offset + visible_height).min(total_lines);
    let lines: Vec<Line> = (scroll_offset..end)
        .map(|i| highlight_line(&app.log_buffer[i], query_lower))
        .collect();

    let history_label = if app.log_search_pending && app.log_loading_history {
        " [Searching...]"
    } else if app.log_loading_history {
        " [Loading...]"
    } else {
        ""
    };
    let search_label = if app.mode == AppMode::LogSearchInput {
        format!(" /{}_", app.log_search_input)
    } else if !app.log_search_query.is_empty() {
        format!(" /{}", app.log_search_query)
    } else {
        String::new()
    };
    let title = format!(
        "Logs [{} lines] [{}]{}{}",
        total_lines, mode_label, history_label, search_label,
    );

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .style(STYLE_NORMAL);

    f.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span_texts<'a>(line: &'a Line<'a>) -> Vec<&'a str> {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn is_highlighted(span: &Span) -> bool {
        span.style == STYLE_SEARCH_MATCH
    }

    #[test]
    fn empty_needle_returns_raw() {
        let line = highlight_line("hello world", "");
        assert_eq!(line, Line::raw("hello world"));
    }

    #[test]
    fn no_match_returns_raw() {
        let line = highlight_line("hello world", "xyz");
        assert_eq!(line, Line::raw("hello world"));
    }

    #[test]
    fn match_at_start() {
        let line = highlight_line("error: something", "error");
        assert_eq!(span_texts(&line), vec!["error", ": something"]);
        assert!(is_highlighted(&line.spans[0]));
        assert!(!is_highlighted(&line.spans[1]));
    }

    #[test]
    fn match_at_end() {
        let line = highlight_line("found an error", "error");
        assert_eq!(span_texts(&line), vec!["found an ", "error"]);
        assert!(!is_highlighted(&line.spans[0]));
        assert!(is_highlighted(&line.spans[1]));
    }

    #[test]
    fn multiple_matches() {
        let line = highlight_line("err foo err bar err", "err");
        assert_eq!(span_texts(&line), vec!["err", " foo ", "err", " bar ", "err"]);
        assert!(is_highlighted(&line.spans[0]));
        assert!(!is_highlighted(&line.spans[1]));
        assert!(is_highlighted(&line.spans[2]));
    }

    #[test]
    fn case_insensitive() {
        let line = highlight_line("ERROR and Error", "error");
        assert_eq!(span_texts(&line), vec!["ERROR", " and ", "Error"]);
        assert!(is_highlighted(&line.spans[0]));
        assert!(is_highlighted(&line.spans[2]));
    }

    #[test]
    fn empty_text() {
        let line = highlight_line("", "err");
        assert_eq!(line, Line::raw(""));
    }

    #[test]
    fn needle_longer_than_text() {
        let line = highlight_line("ab", "abcdef");
        assert_eq!(line, Line::raw("ab"));
    }

    #[test]
    fn exact_match() {
        let line = highlight_line("err", "err");
        assert_eq!(span_texts(&line), vec!["err"]);
        assert!(is_highlighted(&line.spans[0]));
    }
}
