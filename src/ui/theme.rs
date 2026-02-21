use ratatui::style::{Color, Modifier, Style};

pub const COLOR_TEXT: Color = Color::White;
pub const COLOR_HIGHLIGHT: Color = Color::Cyan;

pub const COLOR_STATUS_RUNNING: Color = Color::Green;
pub const COLOR_STATUS_PENDING: Color = Color::Yellow;
pub const COLOR_STATUS_ERROR: Color = Color::Red;
pub const COLOR_STATUS_TERMINATING: Color = Color::Magenta;
pub const COLOR_STATUS_SUCCEEDED: Color = Color::Cyan;
pub const COLOR_VERSION: Color = Color::DarkGray;

pub const STYLE_NORMAL: Style = Style::new().fg(COLOR_TEXT);
pub const STYLE_HIGHLIGHT: Style = Style::new()
    .fg(Color::Black)
    .bg(COLOR_HIGHLIGHT)
    .add_modifier(Modifier::BOLD);

pub const STYLE_SEARCH_MATCH: Style = Style::new()
    .fg(Color::Black)
    .bg(Color::Yellow)
    .add_modifier(Modifier::BOLD);
