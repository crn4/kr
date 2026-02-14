use crate::app::App;
use crate::models::AppMode;
use crate::ui::components::centered_rect;
use crate::ui::theme::{STYLE_HIGHLIGHT, STYLE_NORMAL};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    text::Span,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

pub fn draw_popup(f: &mut Frame, app: &mut App) {
    let area = centered_rect(50, 50, f.area());
    f.render_widget(Clear, area);

    match app.mode {
        AppMode::ContextSelect => draw_context_popup(f, app, area),
        AppMode::NamespaceSelect => draw_namespace_popup(f, app, area),
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
        // Typing mode: input field + filtered list
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
        // Scroll mode: just the list
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
