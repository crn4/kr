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
    widgets::{Block, Borders, Clear, Paragraph, Tabs},
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Main
            Constraint::Length(1), // Footer
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_main(f, app, chunks[1]);
    draw_footer(f, app, chunks[2]);

    match app.mode {
        AppMode::SecretDecode => secrets_view::draw_decode_modal(f, app),
        AppMode::ContextSelect | AppMode::NamespaceSelect | AppMode::StatusFilter => {
            popup_view::draw_popup(f, app)
        }
        AppMode::ScaleInput => draw_scale_input(f, app),
        AppMode::Confirm => draw_confirm(f, app),
        AppMode::ShellView => shell_view::draw(f, app),
        AppMode::DescribeView => describe_view::draw(f, app),
        _ => {}
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .margin(0)
        .split(area);

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
    f.render_widget(tabs, chunks[0]);

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
        app.current_namespace,
        app.filtered_items.len(),
        filter_part,
        status_part,
    );
    let info = Paragraph::new(info_text).style(STYLE_NORMAL);
    f.render_widget(info, chunks[1]);
}

const SPINNER: &[char] = &['◐', '◓', '◑', '◒'];

fn draw_main(f: &mut Frame, app: &mut App, area: Rect) {
    if !matches!(app.mode, AppMode::LogView | AppMode::LogSearchInput)
        && app.is_loading
        && app.filtered_items.is_empty()
    {
        let resource = match app.active_tab {
            ResourceType::Pod => "pods",
            ResourceType::Deployment => "deployments",
            ResourceType::Secret => "secrets",
        };
        let elapsed = app
            .loading_since
            .map(|t| format!(" ({:.1}s)", t.elapsed().as_secs_f64()))
            .unwrap_or_default();
        let spinner_idx = app
            .loading_since
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
        AppMode::LogView | AppMode::LogSearchInput => logs_view::draw(f, app, area),
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
    let help = match app.mode {
        AppMode::List => match app.active_tab {
            ResourceType::Pod => {
                "q:Quit /:Filter f:Status j/k:Nav g/G:Top/End Space:Sel ^a:All Tab:Next l:Logs s:Shell D:Del d:Desc e:Edit c:Ctx n:NS"
            }
            ResourceType::Deployment => {
                "q:Quit /:Filter j/k:Nav g/G:Top/End PgUp/PgDn Space:Sel ^a:All Tab:Next S:Scale r:Restart D:Del d:Desc e:Edit c:Ctx n:NS"
            }
            ResourceType::Secret => {
                "q:Quit /:Filter j/k:Nav g/G:Top/End PgUp/PgDn Tab:Next Enter/x:Decode c:Ctx n:NS"
            }
        },
        AppMode::FilterInput => "Type to filter | Esc:Cancel | Enter:Confirm",
        AppMode::SecretDecode => "j/k:Scroll | r:Reveal | c:Copy | q/Esc:Close",
        AppMode::LogView => "j/k:Scroll | PgUp/PgDn | g/G:Top/Follow | /:Search n/N:Next/Prev | q/Esc:Back",
        AppMode::LogSearchInput => "Type to search | Enter:Confirm | Esc:Cancel",
        AppMode::ScaleInput => "Enter replica count | Enter:Confirm | Esc:Cancel",
        AppMode::Confirm => "y:Confirm | n/Esc:Cancel",
        AppMode::DescribeView => "j/k:Scroll | PgUp/PgDn | g/G:Top/Bottom | q/Esc:Close",
        AppMode::ShellView => if app.shell_title.starts_with("Edit") {
            "Ctrl+Q:Close editor"
        } else {
            "Ctrl+Q:Close shell"
        },
        AppMode::StatusFilter => "j/k:Nav | Space:Toggle | a:All | Enter:Apply | Esc:Cancel",
        AppMode::ContextSelect => "j/k:Nav | Enter:Select | Esc:Cancel",
        AppMode::NamespaceSelect => {
            if app.namespace_typing {
                "Type namespace | Up/Down:Nav | Enter:Select | Esc:Back"
            } else {
                "j/k:Nav | /:Search | Enter:Select | Esc:Cancel"
            }
        }
    };
    let p = Paragraph::new(help).style(STYLE_NORMAL);
    f.render_widget(p, area);
}

fn draw_scale_input(f: &mut Frame, app: &App) {
    let area = centered_fixed_rect(35, 5, f.area());
    f.render_widget(Clear, area);

    let text = format!("Replicas: {}_", app.scale_input);
    let p = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Scale Deployment")
                .style(STYLE_NORMAL),
        )
        .style(STYLE_NORMAL);
    f.render_widget(p, area);
}

fn draw_confirm(f: &mut Frame, app: &App) {
    let area = centered_fixed_rect(50, 9, f.area());
    f.render_widget(Clear, area);

    let msg = app
        .pending_action
        .as_ref()
        .map(|a| a.message())
        .unwrap_or_else(|| "Confirm action?".to_string());
    let text = format!("{}\n\n[y] Yes  [n] No", msg);
    let p = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Confirm")
                .style(STYLE_NORMAL),
        )
        .style(STYLE_NORMAL);
    f.render_widget(p, area);
}
