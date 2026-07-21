use crate::app::App;
use crate::models::KubeResource;
use crate::ui::components::build_sort_header;
use crate::ui::theme::*;
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Block, Borders, Cell, HighlightSpacing, Paragraph, Row, Table},
};

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let wide = app.wide_pods;
    let sort_col = app.active_sort_column();
    let sort_ind = app.active_sort_direction().indicator();
    let base: &[&str] = if wide {
        &[
            "", "Name", "Ready", "Status", "Restarts", "Age", "IP", "Node", "Image",
        ]
    } else {
        &["", "Name", "Ready", "Status", "Restarts", "Age"]
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

            let KubeResource::Pod(p) = item else {
                return Row::new(vec![Cell::from(marker), Cell::from(item.name())]).height(1);
            };

            let name = p.metadata.name.as_deref().unwrap_or_default();
            let status_obj = p.status.as_ref();
            let phase = status_obj
                .and_then(|s| s.phase.as_deref())
                .unwrap_or_default();

            let restarts = App::pod_restarts(p);
            let ready_count = App::pod_ready_count(p);
            let total_containers = App::pod_total_containers(p);

            let age = crate::utils::get_resource_age(p.metadata.creation_timestamp.as_ref());

            let status_style = match phase {
                "Running" => Style::default().fg(COLOR_STATUS_RUNNING),
                "Pending" => Style::default().fg(COLOR_STATUS_PENDING),
                "Succeeded" => Style::default().fg(COLOR_STATUS_SUCCEEDED),
                "Terminating" => Style::default().fg(COLOR_STATUS_TERMINATING),
                _ => Style::default().fg(COLOR_STATUS_ERROR),
            };

            let marker_style = if selected {
                Style::default().fg(COLOR_STATUS_RUNNING)
            } else {
                STYLE_NORMAL
            };

            let mut cells = vec![
                Cell::from(marker).style(marker_style),
                Cell::from(name),
                Cell::from(format!("{}/{}", ready_count, total_containers)),
                Cell::from(phase).style(status_style),
                Cell::from(restarts.to_string()),
                Cell::from(age),
            ];
            if wide {
                let ip = status_obj.and_then(|s| s.pod_ip.as_deref()).unwrap_or("-");
                let node = p
                    .spec
                    .as_ref()
                    .and_then(|s| s.node_name.as_deref())
                    .unwrap_or("-");
                let images: String = p
                    .spec
                    .as_ref()
                    .map(|s| {
                        s.containers
                            .iter()
                            .map(|c| c.image.as_deref().unwrap_or("-"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                cells.push(Cell::from(ip));
                cells.push(Cell::from(node));
                cells.push(Cell::from(images));
            }
            Row::new(cells).height(1)
        })
        .collect();

    let title: std::borrow::Cow<'static, str> = if app.selected_indices.is_empty() {
        "Pods".into()
    } else {
        format!("Pods ({} selected)", app.selected_indices.len()).into()
    };

    let widths: &[Constraint] = if wide {
        &[
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(16),
            Constraint::Fill(1),
            Constraint::Fill(2),
        ]
    } else {
        &[
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(8),
        ]
    };

    if app.filtered_items.is_empty() && !app.is_active_tab_loading() {
        let msg = if !app.has_namespace() {
            "No namespace selected — press n to choose one"
        } else if app.last_error.is_some() {
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
        let t = Table::new(rows, widths)
            .header(header)
            .block(Block::default().borders(Borders::ALL).title(title))
            .row_highlight_style(STYLE_HIGHLIGHT)
            .highlight_symbol("> ")
            .highlight_spacing(HighlightSpacing::Always);
        f.render_stateful_widget(t, area, &mut app.table_state);
    }
}
