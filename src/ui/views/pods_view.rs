use crate::app::App;
use crate::models::KubeResource;
use crate::ui::theme::*;
use ratatui::{
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Block, Borders, Cell, HighlightSpacing, Paragraph, Row, Table},
    Frame,
};

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let header_cells = ["", "Name", "Ready", "Status", "Restarts", "Age"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(COLOR_HIGHLIGHT)));
    let header = Row::new(header_cells)
        .style(STYLE_NORMAL)
        .height(1)
        .bottom_margin(1);

    let rows: Vec<Row> = app
        .filtered_items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let marker = if app.selected_indices.contains(&idx) {
                "●"
            } else {
                " "
            };

            let KubeResource::Pod(p) = item else {
                return Row::new(vec![
                    Cell::from(marker),
                    Cell::from(item.name().to_owned()),
                ])
                .height(1);
            };

            let name = p.metadata.name.as_deref().unwrap_or_default();
            let status_obj = p.status.as_ref();
            let phase = status_obj
                .and_then(|s| s.phase.as_deref())
                .unwrap_or_default();

            let restarts: i32 = status_obj
                .and_then(|s| {
                    s.container_statuses
                        .as_ref()
                        .map(|c| c.iter().map(|cs| cs.restart_count).sum())
                })
                .unwrap_or(0);

            let ready_count = status_obj
                .and_then(|s| {
                    s.container_statuses
                        .as_ref()
                        .map(|c| c.iter().filter(|cs| cs.ready).count())
                })
                .unwrap_or(0);

            let total_containers = p.spec.as_ref().map(|s| s.containers.len()).unwrap_or(0);

            let age = crate::utils::get_resource_age(p.metadata.creation_timestamp.as_ref());

            let status_style = match phase {
                "Running" => Style::default().fg(COLOR_STATUS_RUNNING),
                "Pending" => Style::default().fg(COLOR_STATUS_PENDING),
                "Succeeded" => Style::default().fg(COLOR_STATUS_SUCCEEDED),
                "Terminating" => Style::default().fg(COLOR_STATUS_TERMINATING),
                _ => Style::default().fg(COLOR_STATUS_ERROR),
            };

            let marker_style = if app.selected_indices.contains(&idx) {
                Style::default().fg(COLOR_STATUS_RUNNING)
            } else {
                STYLE_NORMAL
            };

            Row::new(vec![
                Cell::from(marker).style(marker_style),
                Cell::from(name.to_owned()),
                Cell::from(format!("{}/{}", ready_count, total_containers)),
                Cell::from(phase.to_owned()).style(status_style),
                Cell::from(restarts.to_string()),
                Cell::from(age),
            ])
            .height(1)
        })
        .collect();

    let title = if app.selected_indices.is_empty() {
        "Pods".to_string()
    } else {
        format!("Pods ({} selected)", app.selected_indices.len())
    };

    let t = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(title.clone()))
    .row_highlight_style(STYLE_HIGHLIGHT)
    .highlight_symbol("> ")
    .highlight_spacing(HighlightSpacing::Always);

    if app.filtered_items.is_empty() && !app.is_loading {
        let msg = if app.last_error.is_some() {
            ""
        } else if app.filter_query.is_empty() && app.status_filter.is_empty() {
            "No pods in this namespace"
        } else {
            "No pods match filter"
        };
        let empty = Paragraph::new(msg)
            .style(STYLE_NORMAL)
            .block(Block::default().borders(Borders::ALL).title(title));
        f.render_widget(empty, area);
    } else {
        f.render_stateful_widget(t, area, &mut app.table_state);
    }
}
