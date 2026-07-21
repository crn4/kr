use crate::app::App;
use crate::models::AppMode;
use crate::ui::components::{centered_fixed_rect, centered_rect};
use crate::ui::theme::*;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

pub fn draw_popup(f: &mut Frame, app: &mut App) {
    match app.mode {
        AppMode::ContextSelect | AppMode::NamespaceSelect => {
            let area = centered_rect(50, 50, f.area());
            f.render_widget(Clear, area);
            match app.mode {
                AppMode::ContextSelect => draw_context_popup(f, app, area),
                AppMode::NamespaceSelect => draw_namespace_popup(f, app, area),
                _ => {}
            }
        }
        AppMode::StatusFilter => draw_status_filter_popup(f, app),
        AppMode::PortForwardList => draw_port_forward_list(f, app),
        _ => {}
    }
}

fn draw_context_popup(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let list_items: Vec<ListItem> = app
        .available_contexts
        .iter()
        .map(|ctx| {
            let label = if *ctx == app.current_context {
                format!("{ctx} (current)")
            } else {
                ctx.clone()
            };
            ListItem::new(Span::raw(label))
        })
        .chain(app.teleport.clusters().iter().map(|cluster| {
            ListItem::new(Line::from(vec![
                Span::styled("⚡ ", Style::default().fg(COLOR_TELEPORT)),
                Span::raw(cluster.as_str()),
                Span::styled(" (teleport)", Style::default().fg(COLOR_TELEPORT)),
            ]))
        }))
        .chain(app.teleport.needs_login().then(|| {
            ListItem::new(Span::styled(
                "⚡ Log in to Teleport (tsh login)",
                Style::default().fg(COLOR_TELEPORT),
            ))
        }))
        .collect();

    let list = List::new(list_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Select Context"),
        )
        .highlight_style(STYLE_HIGHLIGHT)
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut app.popup_state);
}

fn draw_namespace_popup(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    if app.namespace_typing {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        let input_text = format!("{}_", app.namespace_input);
        let input = Paragraph::new(input_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Type namespace")
                    .style(STYLE_NORMAL),
            )
            .style(STYLE_NORMAL);
        f.render_widget(input, chunks[0]);

        let list_items: Vec<ListItem> = app
            .filtered_namespaces
            .iter()
            .map(|i| ListItem::new(Span::raw(i)))
            .collect();

        let list = List::new(list_items)
            .block(Block::default().borders(Borders::ALL))
            .highlight_style(STYLE_HIGHLIGHT)
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, chunks[1], &mut app.popup_state);
    } else {
        let list_items: Vec<ListItem> = app
            .filtered_namespaces
            .iter()
            .map(|i| ListItem::new(Span::raw(i)))
            .collect();

        let list = List::new(list_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Select Namespace"),
            )
            .highlight_style(STYLE_HIGHLIGHT)
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, area, &mut app.popup_state);
    }
}

fn status_color(phase: &str) -> ratatui::style::Color {
    match phase {
        "Running" => COLOR_STATUS_RUNNING,
        "Pending" => COLOR_STATUS_PENDING,
        "Succeeded" => COLOR_STATUS_SUCCEEDED,
        "Terminating" => COLOR_STATUS_TERMINATING,
        _ => COLOR_STATUS_ERROR,
    }
}

fn draw_status_filter_popup(f: &mut Frame, app: &mut App) {
    let h = (app.status_filter_items.len() as u16 + 2).max(4);
    let area = centered_fixed_rect(40, h, f.area());
    f.render_widget(Clear, area);

    let list_items: Vec<ListItem> = app
        .status_filter_items
        .iter()
        .enumerate()
        .map(|(i, (phase, count))| {
            let marker = if app.status_filter_selected.contains(&i) {
                "●"
            } else {
                " "
            };
            let line = Line::from(vec![
                Span::styled(
                    format!("{marker} "),
                    Style::default().fg(COLOR_STATUS_RUNNING),
                ),
                Span::styled(phase.as_str(), Style::default().fg(status_color(phase))),
                Span::styled(format!(" ({count})"), STYLE_NORMAL),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(list_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Filter by Status"),
        )
        .highlight_style(STYLE_HIGHLIGHT)
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut app.status_filter_state);
}

fn draw_port_forward_list(f: &mut Frame, app: &mut App) {
    let entries: Vec<String> = app
        .port_forwards
        .iter()
        .map(|pf| {
            let age = crate::utils::format_duration_short(pf.started_at.elapsed().as_secs());
            format!(
                "localhost:{} → {}:{} ({}, {})",
                pf.local_port, pf.pod_name, pf.remote_port, pf.namespace, age
            )
        })
        .collect();

    let max_line = entries.iter().map(|s| s.len()).max().unwrap_or(20);
    let w = (max_line as u16 + 2 + 3).max(30);
    let h = (entries.len() as u16 + 2).max(4);
    let area = centered_fixed_rect(w, h, f.area());
    f.render_widget(Clear, area);

    let list_items: Vec<ListItem> = entries
        .iter()
        .zip(app.port_forwards.iter())
        .map(|(text, pf)| {
            let local_part = format!("localhost:{}", pf.local_port);
            let rest = &text[local_part.len()..];
            ListItem::new(Line::from(vec![
                Span::styled(
                    local_part,
                    Style::default().fg(crate::ui::theme::COLOR_HIGHLIGHT),
                ),
                Span::styled(rest, STYLE_NORMAL),
            ]))
        })
        .collect();

    let list = List::new(list_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Active Forwards"),
        )
        .highlight_style(STYLE_HIGHLIGHT)
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut app.port_forward_list_state);
}
