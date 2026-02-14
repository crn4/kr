use crate::app::App;
use crate::models::KubeResource;
use crate::ui::components::centered_rect;
use crate::ui::theme::*;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Cell, Clear, HighlightSpacing, Row, Table},
    Frame,
};

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let header_cells = ["Name", "Type", "Data Count", "Age"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(COLOR_HIGHLIGHT)));
    let header = Row::new(header_cells)
        .style(STYLE_NORMAL)
        .height(1)
        .bottom_margin(1);

    let rows = app.filtered_items.iter().map(|item| {
        let KubeResource::Secret(s) = item else {
            return Row::new(vec![Cell::from(item.name().to_owned())]).height(1);
        };

        let name = s.metadata.name.as_deref().unwrap_or_default();
        let type_ = s.type_.as_deref().unwrap_or_default();
        let count = s.data.as_ref().map(|d| d.len()).unwrap_or(0);
        let age = crate::utils::get_resource_age(s.metadata.creation_timestamp.as_ref());

        Row::new(vec![
            Cell::from(name.to_owned()),
            Cell::from(type_.to_owned()),
            Cell::from(count.to_string()),
            Cell::from(age),
        ])
        .height(1)
    });

    let t = Table::new(
        rows,
        [
            Constraint::Fill(1),
            Constraint::Length(25),
            Constraint::Length(12),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title("Secrets"))
    .row_highlight_style(STYLE_HIGHLIGHT)
    .highlight_symbol("> ")
    .highlight_spacing(HighlightSpacing::Always);

    if app.filtered_items.is_empty() && !app.is_loading {
        let msg = if app.last_error.is_some() {
            "" // error shown in footer
        } else if app.filter_query.is_empty() {
            "No secrets in this namespace"
        } else {
            "No secrets match filter"
        };
        let empty = ratatui::widgets::Paragraph::new(msg)
            .style(STYLE_NORMAL)
            .block(Block::default().borders(Borders::ALL).title("Secrets"));
        f.render_widget(empty, area);
    } else {
        f.render_stateful_widget(t, area, &mut app.table_state);
    }
}

pub fn draw_decode_modal(f: &mut Frame, app: &mut App) {
    let area = centered_rect(60, 60, f.area());
    f.render_widget(Clear, area);

    let Some(decoded) = &app.selected_secret_decoded else {
        return;
    };

    if decoded.is_empty() {
        let p = ratatui::widgets::Paragraph::new("No data in secret.")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Decoded Secret")
                    .style(STYLE_NORMAL),
            )
            .style(STYLE_NORMAL);
        f.render_widget(p, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from("KEY").style(Style::default().fg(COLOR_HIGHLIGHT)),
        Cell::from("VALUE").style(Style::default().fg(COLOR_HIGHLIGHT)),
    ])
    .height(1)
    .bottom_margin(1);

    let rows: Vec<Row> = decoded
        .iter()
        .map(|(k, v)| {
            let display_val = if app.secret_revealed {
                v.as_str().to_owned()
            } else {
                "********".to_owned()
            };
            Row::new(vec![Cell::from(k.as_str()), Cell::from(display_val)])
        })
        .collect();

    app.secret_table_state.select(Some(app.secret_scroll));

    let t = Table::new(
        rows,
        [Constraint::Percentage(30), Constraint::Percentage(70)],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Decoded Secret")
            .style(STYLE_NORMAL),
    )
    .row_highlight_style(
        Style::default()
            .fg(COLOR_HIGHLIGHT)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("> ");

    f.render_stateful_widget(t, area, &mut app.secret_table_state);
}
