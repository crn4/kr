use ratatui::style::{Color, Modifier, Style};

pub const COLOR_TEXT: Color = Color::White;
pub const COLOR_HIGHLIGHT: Color = Color::Cyan;

pub const COLOR_STATUS_RUNNING: Color = Color::Green;
pub const COLOR_STATUS_PENDING: Color = Color::Yellow;
pub const COLOR_STATUS_ERROR: Color = Color::Red;
pub const COLOR_STATUS_TERMINATING: Color = Color::Magenta;
pub const COLOR_STATUS_SUCCEEDED: Color = Color::Cyan;
pub const COLOR_VERSION: Color = Color::DarkGray;
pub const COLOR_TELEPORT: Color = Color::Yellow;

pub const STYLE_NORMAL: Style = Style::new().fg(COLOR_TEXT);
pub const STYLE_HIGHLIGHT: Style = Style::new()
    .fg(Color::Black)
    .bg(COLOR_HIGHLIGHT)
    .add_modifier(Modifier::BOLD);

pub const STYLE_SEARCH_MATCH: Style = Style::new()
    .fg(Color::Black)
    .bg(Color::Yellow)
    .add_modifier(Modifier::BOLD);

pub const STYLE_LOG_SELECTION: Style = Style::new()
    .fg(Color::White)
    .bg(Color::Blue)
    .add_modifier(Modifier::BOLD);

pub fn status_color(status: &str) -> Color {
    match status {
        "Running" => COLOR_STATUS_RUNNING,
        "Succeeded" | "Completed" => COLOR_STATUS_SUCCEEDED,
        "Pending" | "ContainerCreating" | "PodInitializing" | "NotReady" => COLOR_STATUS_PENDING,
        "Terminating" => COLOR_STATUS_TERMINATING,
        s if is_init_progress(s) => COLOR_STATUS_PENDING,
        _ => COLOR_STATUS_ERROR,
    }
}

fn is_init_progress(status: &str) -> bool {
    status.strip_prefix("Init:").is_some_and(|rest| {
        rest.split_once('/').is_some_and(|(done, total)| {
            !done.is_empty()
                && !total.is_empty()
                && done
                    .bytes()
                    .chain(total.bytes())
                    .all(|b| b.is_ascii_digit())
        })
    })
}
