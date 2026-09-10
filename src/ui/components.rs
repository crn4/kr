use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Table, TableState};
use std::borrow::Cow;

pub(crate) const TABLE_CHROME_LINES: u16 = 4;

pub(crate) struct TableWindow {
    pub start: usize,
    pub end: usize,
    pub cursor: Option<usize>,
}

impl TableWindow {
    pub(crate) fn new(state: &TableState, len: usize, area: Rect) -> Self {
        let body = area.height.saturating_sub(TABLE_CHROME_LINES) as usize;
        let (start, end, cursor) = visible_window(state.offset(), state.selected(), len, body);
        Self { start, end, cursor }
    }

    pub(crate) fn render(
        &self,
        f: &mut Frame,
        table: Table<'_>,
        area: Rect,
        state: &mut TableState,
    ) {
        let mut window_state =
            TableState::default().with_selected(self.cursor.map(|c| c - self.start));
        f.render_stateful_widget(table, area, &mut window_state);
        *state.offset_mut() = self.start;
        state.select(self.cursor);
    }
}

pub(crate) fn visible_window(
    offset: usize,
    selected: Option<usize>,
    len: usize,
    body: usize,
) -> (usize, usize, Option<usize>) {
    if len == 0 {
        return (0, 0, None);
    }
    let body = body.max(1);
    let last = len - 1;
    let selected = selected.map(|s| s.min(last));

    let mut start = offset.min(last);
    if let Some(s) = selected {
        if s < start {
            start = s;
        } else if s >= start + body {
            start = s + 1 - body;
        }
    }
    start = start.min(len.saturating_sub(body));

    (start, (start + body).min(len), selected)
}

pub fn build_sort_header(
    columns: &[&'static str],
    sort_col: usize,
    sort_indicator: &str,
) -> Vec<Cow<'static, str>> {
    columns
        .iter()
        .enumerate()
        .map(|(i, &h)| {
            if i > 0 && i - 1 == sort_col {
                Cow::Owned(format!("{h}{sort_indicator}"))
            } else {
                Cow::Borrowed(h)
            }
        })
        .collect()
}

pub fn centered_fixed_rect(width: u16, height: u16, r: Rect) -> Rect {
    let w = width.min(r.width);
    let h = height.min(r.height);
    let x = r.x + (r.width.saturating_sub(w)) / 2;
    let y = r.y + (r.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_follows_the_cursor_down() {
        assert_eq!(visible_window(0, Some(40), 500, 20), (21, 41, Some(40)));
    }

    #[test]
    fn window_follows_the_cursor_up() {
        assert_eq!(visible_window(300, Some(3), 500, 20), (3, 23, Some(3)));
    }

    #[test]
    fn window_keeps_a_stable_offset_while_the_cursor_is_inside() {
        assert_eq!(
            visible_window(100, Some(105), 500, 20),
            (100, 120, Some(105))
        );
    }

    #[test]
    fn window_clamps_a_cursor_past_the_end() {
        assert_eq!(visible_window(0, Some(900), 500, 20), (480, 500, Some(499)));
    }

    #[test]
    fn window_never_starts_past_the_last_page() {
        assert_eq!(visible_window(490, None, 500, 20), (480, 500, None));
    }

    #[test]
    fn window_handles_an_empty_list_and_a_zero_height_body() {
        assert_eq!(visible_window(0, Some(0), 0, 20), (0, 0, None));
        assert_eq!(visible_window(0, Some(7), 500, 0), (7, 8, Some(7)));
    }

    #[test]
    fn window_shorter_than_the_body_shows_everything() {
        assert_eq!(visible_window(0, Some(1), 3, 20), (0, 3, Some(1)));
    }

    #[test]
    fn centered_rect_50_50() {
        let parent = Rect::new(0, 0, 100, 100);
        let r = centered_rect(50, 50, parent);
        assert!(r.width > 0);
        assert!(r.height > 0);
        assert!(r.x > 0);
        assert!(r.y > 0);
        let cx = r.x + r.width / 2;
        let cy = r.y + r.height / 2;
        assert!((cx as i32 - 50).abs() <= 2);
        assert!((cy as i32 - 50).abs() <= 2);
    }

    #[test]
    fn centered_rect_100_100_fills_parent() {
        let parent = Rect::new(0, 0, 80, 40);
        let r = centered_rect(100, 100, parent);
        assert_eq!(r.width, parent.width);
        assert_eq!(r.height, parent.height);
    }

    #[test]
    fn centered_rect_small_parent() {
        let parent = Rect::new(0, 0, 10, 10);
        let r = centered_rect(60, 60, parent);
        assert!(r.width <= parent.width);
        assert!(r.height <= parent.height);
    }

    #[test]
    fn sort_header_marks_correct_column() {
        let cols = &["", "Name", "Status", "Age"];
        let result = build_sort_header(cols, 1, " ▲");
        assert!(matches!(&result[0], Cow::Borrowed("")));
        assert!(matches!(&result[1], Cow::Borrowed("Name")));
        assert_eq!(result[2], "Status ▲");
        assert!(matches!(&result[2], Cow::Owned(_)));
        assert!(matches!(&result[3], Cow::Borrowed("Age")));
    }

    #[test]
    fn sort_header_first_data_column() {
        let cols = &["", "Name", "Age"];
        let result = build_sort_header(cols, 0, " ▼");
        assert_eq!(result[1], "Name ▼");
        assert!(matches!(&result[2], Cow::Borrowed("Age")));
    }

    #[test]
    fn sort_header_marker_column_never_decorated() {
        let cols = &["", "Name"];
        let result = build_sort_header(cols, 0, " ▲");
        assert!(matches!(&result[0], Cow::Borrowed("")));
    }
}
