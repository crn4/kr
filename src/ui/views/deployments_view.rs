use crate::app::App;
use crate::models::KubeResource;
use crate::ui::theme::*;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Cell, HighlightSpacing, Paragraph, Row, Table},
    Frame,
};

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let header_cells = ["", "Name", "Ready", "Up-to-date", "Available", "Age"]
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

            let KubeResource::Deployment(d) = item else {
                return Row::new(vec![
                    Cell::from(marker),
                    Cell::from(item.name().to_owned()),
                ]);
            };

            let name = d.metadata.name.as_deref().unwrap_or_default();
            let status = d.status.as_ref();
            let replicas = status.map_or(0, |s| s.replicas.unwrap_or(0));
            let ready = status.map_or(0, |s| s.ready_replicas.unwrap_or(0));
            let updated = status.map_or(0, |s| s.updated_replicas.unwrap_or(0));
            let available = status.map_or(0, |s| s.available_replicas.unwrap_or(0));
            let age = crate::utils::get_resource_age(d.metadata.creation_timestamp.as_ref());

            let marker_style = if app.selected_indices.contains(&idx) {
                Style::default().fg(COLOR_STATUS_RUNNING)
            } else {
                STYLE_NORMAL
            };

            Row::new(vec![
                Cell::from(marker).style(marker_style),
                Cell::from(name.to_owned()).style(STYLE_NORMAL.add_modifier(Modifier::BOLD)),
                Cell::from(format!("{}/{}", ready, replicas)),
                Cell::from(updated.to_string()),
                Cell::from(available.to_string()),
                Cell::from(age),
            ])
            .height(1)
            .style(STYLE_NORMAL)
        })
        .collect();

    let title = if app.selected_indices.is_empty() {
        "Deployments".to_string()
    } else {
        format!("Deployments ({} selected)", app.selected_indices.len())
    };

    let t = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(10),
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
            "" // error shown in footer
        } else if app.filter_query.is_empty() {
            "No deployments in this namespace"
        } else {
            "No deployments match filter"
        };
        let empty = Paragraph::new(msg)
            .style(STYLE_NORMAL)
            .block(Block::default().borders(Borders::ALL).title(title));
        f.render_widget(empty, area);
    } else {
        f.render_stateful_widget(t, area, &mut app.table_state);
    }
}
