pub mod components;
pub mod theme;
pub mod views;

use crate::app::App;
use crate::models::{AppMode, ResourceType};
use crate::ui::components::centered_fixed_rect;
use crate::ui::theme::*;
use crate::ui::views::*;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Tabs, Wrap},
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_main(f, app, chunks[1]);
    draw_footer(f, app, chunks[2]);

    match app.mode {
        AppMode::SecretDecode => secrets_view::draw_decode_modal(f, app),
        AppMode::ContextSelect
        | AppMode::NamespaceSelect
        | AppMode::StatusFilter
        | AppMode::PortForwardList => popup_view::draw_popup(f, app),
        AppMode::ScaleInput => draw_scale_input(f, app),
        AppMode::PortForwardInput => draw_port_forward_input(f, app),
        AppMode::Confirm => draw_confirm(f, app),
        AppMode::ShellView => shell_view::draw(f, app),
        AppMode::DescribeView => describe_view::draw(f, app),
        AppMode::Help => help_view::draw(f, app),
        _ => {}
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .margin(0)
        .split(area);

    let version_label = concat!("v", env!("CARGO_PKG_VERSION"), " ");
    let version_width = version_label.len() as u16;
    let tab_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(version_width)])
        .split(chunks[0]);

    let titles = ["Pods", "Deployments", "Secrets"]
        .iter()
        .map(|t| Line::from(Span::styled(*t, Style::default().fg(COLOR_TEXT))))
        .collect::<Vec<Line>>();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::NONE))
        .highlight_style(STYLE_HIGHLIGHT)
        .select(match app.active_tab {
            ResourceType::Pod => 0,
            ResourceType::Deployment => 1,
            ResourceType::Secret => 2,
        });
    f.render_widget(tabs, tab_row[0]);

    let version = Paragraph::new(version_label).style(Style::default().fg(COLOR_VERSION));
    f.render_widget(version, tab_row[1]);

    let filter_part = if app.filter_query.is_empty() {
        String::new()
    } else if app.mode == AppMode::FilterInput {
        format!(" | Filter: {}_", app.filter_query)
    } else {
        format!(" | Filter: {}", app.filter_query)
    };

    let status_part = if app.status_filter.is_empty() {
        String::new()
    } else {
        let mut statuses: Vec<&str> = app.status_filter.iter().map(|s| s.as_str()).collect();
        statuses.sort_unstable();
        format!(" | Status: {}", statuses.join(", "))
    };

    let info_text = format!(
        " Ctx: {} | NS: {} | Items: {}{}{}",
        app.current_context,
        if app.has_namespace() {
            app.current_namespace.as_str()
        } else {
            "<none>"
        },
        app.filtered_items.len(),
        filter_part,
        status_part,
    );
    let info = Paragraph::new(info_text).style(STYLE_NORMAL);
    f.render_widget(info, chunks[1]);
}

const SPINNER: &[char] = &['◐', '◓', '◑', '◒'];

fn draw_main(f: &mut Frame, app: &mut App, area: Rect) {
    if !matches!(
        app.mode,
        AppMode::LogView | AppMode::LogSearchInput | AppMode::LogVisualSelect
    ) && app.is_active_tab_loading()
        && app.filtered_items.is_empty()
    {
        let resource = match app.active_tab {
            ResourceType::Pod => "pods",
            ResourceType::Deployment => "deployments",
            ResourceType::Secret => "secrets",
        };
        let elapsed = app
            .active_tab_loading_since()
            .map(|t| format!(" ({:.1}s)", t.elapsed().as_secs_f64()))
            .unwrap_or_default();
        let spinner_idx = app
            .active_tab_loading_since()
            .map(|t| (t.elapsed().as_millis() / 250) as usize % SPINNER.len())
            .unwrap_or(0);
        let label = format!(
            " {} Loading {} in {}...{}",
            SPINNER[spinner_idx], resource, app.current_namespace, elapsed,
        );
        let p = Paragraph::new(label)
            .style(STYLE_NORMAL)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(p, area);
        return;
    }
    match app.mode {
        AppMode::LogView | AppMode::LogSearchInput | AppMode::LogVisualSelect => {
            logs_view::draw(f, app, area)
        }
        _ => match app.active_tab {
            ResourceType::Pod => pods_view::draw(f, app, area),
            ResourceType::Deployment => deployments_view::draw(f, app, area),
            ResourceType::Secret => secrets_view::draw(f, app, area),
        },
    }
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    if let Some(err) = &app.last_error {
        let p = Paragraph::new(format!(" ERROR: {}", err))
            .style(Style::default().fg(ratatui::style::Color::Red));
        f.render_widget(p, area);
        return;
    }
    if let Some(msg) = &app.last_success {
        let p = Paragraph::new(format!(" OK: {}", msg))
            .style(Style::default().fg(ratatui::style::Color::Green));
        f.render_widget(p, area);
        return;
    }
    let hint = match app.mode {
        AppMode::FilterInput => " Type to filter | Enter:Confirm | Esc:Cancel",
        AppMode::LogSearchInput => " Type to search | Enter:Confirm | Esc:Cancel",
        AppMode::ScaleInput => " Replicas (0-1000) | Enter:Confirm | Esc:Cancel",
        AppMode::PortForwardInput => " Port (8080:80 or 80) | Enter:Confirm | Esc:Cancel",
        AppMode::Confirm => " y:Confirm | n/Esc:Cancel",
        AppMode::LogView => " q:Back  /:Search  V:Select  ?:Help",
        AppMode::LogVisualSelect => " j/k:Extend  g/G:Top/Bot  y:Copy  Esc:Cancel",
        AppMode::DescribeView | AppMode::SecretDecode => " q:Close  ?:Help",
        AppMode::Help => " q/Esc:Close  j/k:Scroll",
        AppMode::PortForwardList => " j/k:Nav  d:Stop  Esc:Close",
        _ => " q:Quit  /:Filter  ?:Help",
    };
    let pf_count = app.port_forwards.len();
    let mut spans = vec![Span::styled(hint, STYLE_NORMAL)];
    if pf_count > 0 {
        let pf_style = Style::default()
            .fg(ratatui::style::Color::Green)
            .add_modifier(ratatui::style::Modifier::BOLD);
        spans.push(Span::styled(format!("  P:Fwd({})", pf_count), pf_style));
    }
    let p = Paragraph::new(Line::from(spans));
    f.render_widget(p, area);
}

fn draw_scale_input(f: &mut Frame, app: &App) {
    let area = centered_fixed_rect(35, 5, f.area());
    f.render_widget(Clear, area);

    let count = app.scale_targets.len();
    let title: std::borrow::Cow<'static, str> = if count > 1 {
        format!("Scale {} Deployments", count).into()
    } else {
        "Scale Deployment".into()
    };
    let text = format!("Replicas: {}_", app.scale_input);
    let p = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(STYLE_NORMAL),
        )
        .style(STYLE_NORMAL);
    f.render_widget(p, area);
}

fn draw_port_forward_input(f: &mut Frame, app: &App) {
    let area = centered_fixed_rect(40, 5, f.area());
    f.render_widget(Clear, area);

    let text = format!("Port: {}_", app.port_forward_input);
    let p = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Port Forward")
                .style(STYLE_NORMAL),
        )
        .style(STYLE_NORMAL);
    f.render_widget(p, area);
}

fn draw_confirm(f: &mut Frame, app: &App) {
    let msg = app
        .pending_action
        .as_ref()
        .map(|a| a.message())
        .unwrap_or_else(|| "Confirm action?".to_string());
    let frame = f.area();
    let (w, h) = confirm_size(&msg, frame.width, frame.height);
    let area = centered_fixed_rect(w, h, frame);
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Confirm")
        .style(STYLE_NORMAL);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    f.render_widget(
        Paragraph::new(msg.as_str())
            .wrap(Wrap { trim: false })
            .style(STYLE_NORMAL),
        body,
    );
    f.render_widget(
        Paragraph::new("[y] Yes  [n] No").style(STYLE_NORMAL),
        footer,
    );
}

pub(crate) fn confirm_size(msg: &str, frame_width: u16, frame_height: u16) -> (u16, u16) {
    const BORDERS: u16 = 2;
    const PADDING: u16 = 2;
    const FOOTER: u16 = 2;

    let width_of = |line: &str| u16::try_from(Span::raw(line).width()).unwrap_or(u16::MAX);

    let widest = msg.lines().map(width_of).max().unwrap_or(15);
    let max_width = frame_width.saturating_sub(2).max(30);
    let w = widest
        .saturating_add(BORDERS + PADDING)
        .clamp(30, max_width);

    let inner = w.saturating_sub(BORDERS).max(1);
    let wrapped: u16 = msg
        .lines()
        .map(|l| width_of(l).div_ceil(inner).max(1))
        .sum();

    let h = wrapped
        .saturating_add(BORDERS)
        .saturating_add(FOOTER)
        .clamp(7, frame_height.max(7));
    (w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_size_fits_a_short_message() {
        let (w, h) = confirm_size("Delete pod 'nginx'?", 120, 40);
        assert_eq!(w, 30);
        assert_eq!(h, 7);
    }

    #[test]
    fn confirm_size_grows_past_the_minimum_for_a_long_line() {
        let msg = format!("Delete 3 deployments?\n{}", "a".repeat(60));
        let (w, _) = confirm_size(&msg, 120, 40);
        assert_eq!(w, 64);
    }

    #[test]
    fn confirm_size_never_exceeds_the_frame() {
        let long = "x".repeat(500);
        let (w, _) = confirm_size(&long, 80, 40);
        assert!(w <= 80, "popup {w} wider than the 80-column frame");
    }

    fn render_confirm_with(
        names: usize,
        name_len: usize,
        frame_w: u16,
        frame_h: u16,
    ) -> Vec<String> {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let list: Vec<String> = (0..names)
            .map(|i| format!("pod-{i}-{}", "x".repeat(name_len)))
            .collect();
        let mut app = crate::app::App::new_test();
        app.pending_action = Some(crate::models::PendingAction::DeleteResource {
            resource: crate::models::ResourceType::Pod,
            names: list,
        });

        let mut terminal = Terminal::new(TestBackend::new(frame_w, frame_h)).unwrap();
        terminal.draw(|f| draw_confirm(f, &app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        (0..frame_h)
            .map(|y| {
                (0..frame_w)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    fn render_confirm(names: usize, frame_w: u16, frame_h: u16) -> Vec<String> {
        render_confirm_with(names, names % 7, frame_w, frame_h)
    }

    #[tokio::test]
    async fn confirm_prompt_survives_a_message_taller_than_the_frame() {
        let rendered = render_confirm_with(20, 200, 80, 40);
        assert!(
            rendered.iter().any(|row| row.contains("[y] Yes  [n] No")),
            "a message that overflows the frame pushed the prompt off screen"
        );
    }

    #[tokio::test]
    async fn confirm_prompt_survives_a_large_multi_select() {
        for names in [1, 40, 120, 200] {
            let rendered = render_confirm(names, 80, 40);
            assert!(
                rendered.iter().any(|row| row.contains("[y] Yes  [n] No")),
                "{names} names: the prompt is not on screen"
            );
        }
    }

    #[tokio::test]
    async fn confirm_lists_names_up_to_the_cap() {
        let rendered = render_confirm(120, 80, 40).join(" ");
        assert!(rendered.contains("Delete 120 pods?"));
        assert!(rendered.contains("and 100 more"));
    }

    #[test]
    fn confirm_size_reserves_rows_for_wrapped_lines() {
        let names = (0..40)
            .map(|i| format!("pod-{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let msg = format!("Delete 40 pods?\n{names}");
        let (w, h) = confirm_size(&msg, 80, 40);
        let inner = w.saturating_sub(2);
        let wrapped = u16::try_from(names.len()).unwrap().div_ceil(inner) + 1;
        assert_eq!(h, wrapped + 4, "borders plus a blank plus the prompt row");
    }

    #[test]
    fn confirm_size_measures_display_width_not_bytes() {
        let (ascii, _) = confirm_size(&"a".repeat(40), 120, 40);
        let (multibyte, _) = confirm_size(&"é".repeat(40), 120, 40);
        assert_eq!(ascii, multibyte);
    }
}
