use crate::app::App;
use crate::models::KubeResource;
use crate::ui::components::build_sort_header;
use crate::ui::theme::*;
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Cell, HighlightSpacing, Paragraph, Row, Table},
};

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let wide = app.wide_deployments;
    let sort_col = app.active_sort_column();
    let sort_ind = app.active_sort_direction().indicator();
    let base: &[&str] = if wide {
        &[
            "",
            "Name",
            "Ready",
            "Up-to-date",
            "Available",
            "Age",
            "Strategy",
            "Images",
        ]
    } else {
        &["", "Name", "Ready", "Up-to-date", "Available", "Age"]
    };
    let cols = build_sort_header(base, sort_col, sort_ind);
    let header_cells = cols
        .iter()
        .map(|h| Cell::from(h.as_ref()).style(Style::default().fg(COLOR_HIGHLIGHT)));

    let header = Row::new(header_cells)
        .style(STYLE_NORMAL)
        .height(1)
        .bottom_margin(1);

    let rows: Vec<Row> = app
        .filtered_items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let selected = app.selected_indices.contains(&idx);
            let marker = if selected { "●" } else { " " };

            let KubeResource::Deployment(d) = item else {
                return Row::new(vec![Cell::from(marker), Cell::from(item.name())]);
            };

            let name = d.metadata.name.as_deref().unwrap_or_default();
            let status = d.status.as_ref();
            let replicas = status.map_or(0, |s| s.replicas.unwrap_or(0));
            let ready = status.map_or(0, |s| s.ready_replicas.unwrap_or(0));
            let updated = status.map_or(0, |s| s.updated_replicas.unwrap_or(0));
            let available = status.map_or(0, |s| s.available_replicas.unwrap_or(0));
            let age = crate::utils::get_resource_age(d.metadata.creation_timestamp.as_ref());

            let marker_style = if selected {
                Style::default().fg(COLOR_STATUS_RUNNING)
            } else {
                STYLE_NORMAL
            };

            let mut cells = vec![
                Cell::from(marker).style(marker_style),
                Cell::from(name).style(STYLE_NORMAL.add_modifier(Modifier::BOLD)),
                Cell::from(format!("{}/{}", ready, replicas)),
                Cell::from(updated.to_string()),
                Cell::from(available.to_string()),
                Cell::from(age),
            ];
            if wide {
                let strategy = d
                    .spec
                    .as_ref()
                    .and_then(|s| s.strategy.as_ref())
                    .and_then(|s| s.type_.as_deref())
                    .unwrap_or("-");
                let images: String = d
                    .spec
                    .as_ref()
                    .and_then(|s| s.template.spec.as_ref())
                    .map(|ps| {
                        ps.containers
                            .iter()
                            .map(|c| c.image.as_deref().unwrap_or("-"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                cells.push(Cell::from(strategy));
                cells.push(Cell::from(images));
            }
            Row::new(cells).height(1).style(STYLE_NORMAL)
        })
        .collect();

    let title: std::borrow::Cow<'static, str> = if app.selected_indices.is_empty() {
        "Deployments".into()
    } else {
        format!("Deployments ({} selected)", app.selected_indices.len()).into()
    };

    let widths: &[Constraint] = if wide {
        &[
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(14),
            Constraint::Fill(2),
        ]
    } else {
        &[
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(8),
        ]
    };

    if app.filtered_items.is_empty() && !app.is_active_tab_loading() {
        let msg = if !app.has_namespace() {
            "No namespace selected — press n to choose one"
        } else if app.last_error.is_some() {
            ""
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
        let t = Table::new(rows, widths)
            .header(header)
            .block(Block::default().borders(Borders::ALL).title(title))
            .row_highlight_style(STYLE_HIGHLIGHT)
            .highlight_symbol("> ")
            .highlight_spacing(HighlightSpacing::Always);
        f.render_stateful_widget(t, area, &mut app.table_state);
    }
}
