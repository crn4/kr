use ratatui::layout::{Constraint, Direction, Layout, Rect};

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
}
