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

    let body = area
        .height
        .saturating_sub(crate::ui::components::TABLE_CHROME_LINES) as usize;
    let (window_start, window_end, cursor) = crate::ui::components::visible_window(
        app.table_state.offset(),
        app.table_state.selected(),
        app.filtered_items.len(),
        body,
    );

    let rows: Vec<Row> = app.filtered_items[window_start..window_end]
        .iter()
        .map(|item| {
            let selected = app.selected_names.contains(item.name());
            let marker = if selected { "●" } else { " " };

            let KubeResource::Pod(p) = item else {
                return Row::new(vec![Cell::from(marker), Cell::from(item.name())]).height(1);
            };

            let name = p.metadata.name.as_deref().unwrap_or_default();
            let status_obj = p.status.as_ref();
            let status = App::pod_display_status(p);
            let status_style = Style::default().fg(crate::ui::theme::status_color(&status));

            let restarts = App::pod_restarts(p);
            let ready_count = App::pod_ready_count(p);
            let total_containers = App::pod_total_containers(p);

            let age = crate::utils::get_resource_age(p.metadata.creation_timestamp.as_ref());

            let marker_style = if selected {
                Style::default().fg(COLOR_STATUS_RUNNING)
            } else {
                STYLE_NORMAL
            };

            let mut cells = vec![
                Cell::from(marker).style(marker_style),
                Cell::from(name),
                Cell::from(format!("{}/{}", ready_count, total_containers)),
                Cell::from(status).style(status_style),
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

    let title: std::borrow::Cow<'static, str> = if app.selected_names.is_empty() {
        "Pods".into()
    } else {
        format!("Pods ({} selected)", app.selected_names.len()).into()
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
        let mut window_state =
            ratatui::widgets::TableState::default().with_selected(cursor.map(|c| c - window_start));
        f.render_stateful_widget(t, area, &mut window_state);
        *app.table_state.offset_mut() = window_start;
        app.table_state.select(cursor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn app_with_pods(n: usize, cursor: Option<usize>) -> App {
        use k8s_openapi::api::core::v1::Pod;
        use std::sync::Arc;

        let mut app = App::new_test();
        app.active_tab = crate::models::ResourceType::Pod;
        app.items = (0..n)
            .map(|i| {
                let mut pod = Pod::default();
                pod.metadata.name = Some(format!("pod-{i:04}"));
                KubeResource::Pod(Arc::new(pod))
            })
            .collect();
        app.update_filter();
        app.table_state.select(cursor);
        app
    }

    fn rendered_rows(app: &mut App, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(60, height)).unwrap();
        terminal.draw(|f| draw(f, app, f.area())).unwrap();
        let buf = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| (0..60).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect()
    }

    #[tokio::test]
    async fn the_cursor_row_is_rendered_far_down_a_long_list() {
        let mut app = app_with_pods(500, Some(400));
        let rendered = rendered_rows(&mut app, 20).join("\n");
        assert!(rendered.contains("pod-0400"), "{rendered}");
        assert!(!rendered.contains("pod-0000"));
    }

    #[tokio::test]
    async fn scrolling_back_up_follows_the_cursor() {
        let mut app = app_with_pods(500, Some(400));
        rendered_rows(&mut app, 20);
        app.table_state.select(Some(3));
        let rendered = rendered_rows(&mut app, 20).join("\n");
        assert!(rendered.contains("pod-0003"), "{rendered}");
        assert!(!rendered.contains("pod-0400"));
    }

    #[tokio::test]
    async fn a_shrinking_list_pulls_the_window_back_to_the_last_page() {
        let mut app = app_with_pods(500, Some(499));
        rendered_rows(&mut app, 50);
        assert_eq!(app.table_state.offset(), 500 - 46);

        app.items.truncate(100);
        app.update_filter();
        let rendered = rendered_rows(&mut app, 50).join("\n");

        assert_eq!(app.table_state.selected(), Some(99));
        assert_eq!(app.table_state.offset(), 100 - 46);
        assert!(rendered.contains("pod-0054"), "{rendered}");
        assert!(rendered.contains("pod-0099"));
    }

    #[tokio::test]
    async fn sorting_returns_to_the_top_and_keeps_the_multi_select() {
        let mut app = app_with_pods(500, Some(400));
        app.selected_names.insert("pod-0007".into());
        rendered_rows(&mut app, 20);
        assert!(app.table_state.offset() > 0);

        app.cycle_sort_column();

        assert_eq!(app.table_state.offset(), 0);
        assert_eq!(app.table_state.selected(), None);
        assert!(
            app.selected_names.contains("pod-0007"),
            "sorting must not drop a name-keyed selection"
        );
    }

    #[tokio::test]
    async fn a_short_list_renders_from_the_top() {
        let mut app = app_with_pods(3, Some(0));
        let rendered = rendered_rows(&mut app, 20).join("\n");
        for name in ["pod-0000", "pod-0001", "pod-0002"] {
            assert!(rendered.contains(name), "{name} missing:\n{rendered}");
        }
    }

    #[tokio::test]
    async fn a_tiny_terminal_still_renders_the_cursor() {
        let mut app = app_with_pods(500, Some(250));
        let rendered = rendered_rows(&mut app, 6).join("\n");
        assert!(rendered.contains("pod-0250"), "{rendered}");
    }
}
