use crate::app::App;
use crate::models::{AppMode, ResourceType};
use crate::ui::components::centered_fixed_rect;
use crate::ui::theme::*;
use ratatui::{
    Frame,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

const STYLE_KEY: Style = Style::new()
    .fg(COLOR_HIGHLIGHT)
    .add_modifier(Modifier::BOLD);

const STYLE_SECTION: Style = Style::new()
    .fg(COLOR_HIGHLIGHT)
    .add_modifier(Modifier::UNDERLINED);

const MAX_WIDTH: u16 = 52;
const BORDER_CHROME: u16 = 2;

type Binding = (&'static str, &'static str);
type Section = (&'static str, &'static [Binding]);

static LIST_NAVIGATION: Section = ("Navigation", &[
    ("j/k ↑/↓     ", "Navigate up/down"),
    ("g / G        ", "Top / Bottom"),
    ("PgUp/PgDn    ", "Page scroll"),
]);

static LIST_SELECTION: Section = ("Selection", &[
    ("Space        ", "Toggle select"),
    ("Ctrl+A       ", "Select/deselect all"),
]);

static LIST_VIEW: Section = ("View", &[
    ("/            ", "Filter"),
    ("o / O        ", "Cycle sort / Toggle direction"),
    ("Tab/S+Tab    ", "Next/Previous tab"),
    ("w            ", "Toggle wide/compact"),
]);

static LIST_ACTIONS: Section = ("Actions", &[
    ("d            ", "Describe resource"),
    ("e            ", "Edit resource"),
    ("D / Del      ", "Delete resource"),
]);

static LIST_GLOBAL: Section = ("Global", &[
    ("c            ", "Switch context"),
    ("n            ", "Switch namespace"),
    ("q / Ctrl+C   ", "Quit"),
]);

static POD_ACTIONS: &[Binding] = &[
    ("f            ", "Filter by status"),
    ("l            ", "Stream logs"),
    ("s            ", "Shell into pod"),
];

static DEPLOYMENT_ACTIONS: &[Binding] = &[
    ("Enter        ", "Show pods for deployment"),
    ("S            ", "Scale deployment"),
    ("r            ", "Rollout restart"),
];

static SECRET_ACTIONS: &[Binding] = &[
    ("Enter / x    ", "Decode secret"),
];

static LOG_SECTIONS: &[Section] = &[
    ("Navigation", &[
        ("j/k ↑/↓     ", "Scroll up/down"),
        ("h/l ←/→     ", "Pan left/right"),
        ("PgUp/PgDn    ", "Page scroll"),
        ("g            ", "Jump to top"),
        ("G            ", "Follow (jump to bottom)"),
        ("0            ", "Pan to left edge"),
    ]),
    ("Search", &[
        ("/            ", "Search"),
        ("n            ", "Next match (older)"),
        ("N            ", "Prev match (newer)"),
        ("Esc          ", "Clear search"),
    ]),
    ("", &[
        ("q / Esc      ", "Back to list"),
    ]),
];

static DESCRIBE_SECTIONS: &[Section] = &[
    ("Navigation", &[
        ("j/k ↑/↓     ", "Scroll up/down"),
        ("h/l ←/→     ", "Pan left/right"),
        ("PgUp/PgDn    ", "Page scroll"),
        ("g / G        ", "Top / Bottom"),
        ("0            ", "Pan to left edge"),
    ]),
    ("", &[
        ("q / Esc      ", "Close"),
    ]),
];

static SECRET_SECTIONS: &[Section] = &[
    ("", &[
        ("j/k ↑/↓     ", "Scroll up/down"),
        ("r            ", "Reveal/hide values"),
        ("c            ", "Copy value to clipboard"),
        ("q / Esc      ", "Close"),
    ]),
];

pub fn draw(f: &mut Frame, app: &App) {
    let sections = sections_for_mode(app);
    let content_lines = count_lines(sections);
    let popup_height = (content_lines as u16 + BORDER_CHROME).min(f.area().height);
    let area = centered_fixed_rect(MAX_WIDTH, popup_height, f.area());
    f.render_widget(Clear, area);

    let visible = area.height.saturating_sub(BORDER_CHROME) as usize;
    let max_scroll = content_lines.saturating_sub(visible);
    let scroll = app.help_scroll.min(max_scroll);

    let lines = build_lines(sections, scroll, visible);

    let title = help_title(app);

    let mut title_spans = vec![Span::raw(title)];
    if scroll > 0 {
        title_spans.push(Span::raw(" ↑"));
    }
    if max_scroll > 0 && scroll < max_scroll {
        title_spans.push(Span::raw(" ↓"));
    }

    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Line::from(title_spans))
                .style(STYLE_NORMAL),
        )
        .style(STYLE_NORMAL);
    f.render_widget(p, area);
}

fn help_title(app: &App) -> &'static str {
    match app.help_return_mode {
        AppMode::LogView => "Help — Logs",
        AppMode::DescribeView => "Help — Describe",
        AppMode::SecretDecode => "Help — Secret",
        AppMode::List => match app.active_tab {
            ResourceType::Pod => "Help — Pods",
            ResourceType::Deployment => "Help — Deployments",
            ResourceType::Secret => "Help — Secrets",
        },
        _ => "Help",
    }
}

fn sections_for_mode(app: &App) -> &'static [Section] {
    match app.help_return_mode {
        AppMode::LogView => LOG_SECTIONS,
        AppMode::DescribeView => DESCRIBE_SECTIONS,
        AppMode::SecretDecode => SECRET_SECTIONS,
        _ => list_sections(app),
    }
}

static LIST_SECTIONS_POD: &[Section] = &[
    LIST_NAVIGATION,
    LIST_SELECTION,
    LIST_VIEW,
    ("Pod Actions", POD_ACTIONS),
    LIST_ACTIONS,
    LIST_GLOBAL,
];

static LIST_SECTIONS_DEPLOYMENT: &[Section] = &[
    LIST_NAVIGATION,
    LIST_SELECTION,
    LIST_VIEW,
    ("Deployment Actions", DEPLOYMENT_ACTIONS),
    LIST_ACTIONS,
    LIST_GLOBAL,
];

static LIST_SECTIONS_SECRET: &[Section] = &[
    LIST_NAVIGATION,
    LIST_VIEW,
    ("Secret Actions", SECRET_ACTIONS),
    LIST_GLOBAL,
];

fn list_sections(app: &App) -> &'static [Section] {
    match app.active_tab {
        ResourceType::Pod => LIST_SECTIONS_POD,
        ResourceType::Deployment => LIST_SECTIONS_DEPLOYMENT,
        ResourceType::Secret => LIST_SECTIONS_SECRET,
    }
}

fn count_lines(sections: &[Section]) -> usize {
    let mut total = 0;
    for (i, (name, bindings)) in sections.iter().enumerate() {
        if !name.is_empty() {
            if i > 0 {
                total += 1;
            }
            total += 1;
        }
        total += bindings.len();
    }
    total
}

fn build_lines<'a>(sections: &'a [Section], scroll: usize, visible: usize) -> Vec<Line<'a>> {
    let mut all_lines: Vec<Line<'a>> = Vec::with_capacity(count_lines(sections));
    for (i, (name, bindings)) in sections.iter().enumerate() {
        if !name.is_empty() {
            if i > 0 {
                all_lines.push(Line::default());
            }
            all_lines.push(Line::from(Span::styled(*name, STYLE_SECTION)));
        }
        for (key, desc) in *bindings {
            all_lines.push(Line::from(vec![
                Span::styled(*key, STYLE_KEY),
                Span::styled(*desc, STYLE_NORMAL),
            ]));
        }
    }
    all_lines.into_iter().skip(scroll).take(visible).collect()
}

pub fn max_scroll(app: &App) -> usize {
    let sections = sections_for_mode(app);
    let total = count_lines(sections);
    let height = crossterm::terminal::size()
        .map(|(_, h)| h)
        .unwrap_or(24);
    let popup_height = (total as u16 + BORDER_CHROME).min(height);
    let visible = popup_height.saturating_sub(BORDER_CHROME) as usize;
    total.saturating_sub(visible)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    #[test]
    fn count_lines_log_sections() {
        let total = count_lines(LOG_SECTIONS);
        assert!(total >= 10);
    }

    #[test]
    fn count_lines_describe_sections() {
        let total = count_lines(DESCRIBE_SECTIONS);
        assert!(total >= 6);
    }

    #[test]
    fn count_lines_secret_sections() {
        let total = count_lines(SECRET_SECTIONS);
        assert_eq!(total, 4);
    }

    #[tokio::test]
    async fn list_sections_differ_by_tab() {
        let mut app = App::new_test();
        app.help_return_mode = AppMode::List;

        app.active_tab = ResourceType::Pod;
        let pod_count = count_lines(sections_for_mode(&app));

        app.active_tab = ResourceType::Deployment;
        let deploy_count = count_lines(sections_for_mode(&app));

        app.active_tab = ResourceType::Secret;
        let secret_count = count_lines(sections_for_mode(&app));

        assert!(pod_count > secret_count);
        assert_eq!(pod_count, deploy_count);
    }

    #[tokio::test]
    async fn sections_for_log_view() {
        let mut app = App::new_test();
        app.help_return_mode = AppMode::LogView;
        let sections = sections_for_mode(&app);
        assert!(std::ptr::eq(sections, LOG_SECTIONS));
    }

    #[tokio::test]
    async fn sections_for_describe_view() {
        let mut app = App::new_test();
        app.help_return_mode = AppMode::DescribeView;
        let sections = sections_for_mode(&app);
        assert!(std::ptr::eq(sections, DESCRIBE_SECTIONS));
    }

    #[test]
    fn build_lines_respects_scroll_and_visible() {
        let sections: &[Section] = &[
            ("Section", &[
                ("a            ", "desc a"),
                ("b            ", "desc b"),
                ("c            ", "desc c"),
            ]),
        ];
        let lines = build_lines(sections, 1, 2);
        assert_eq!(lines.len(), 2);
    }

    #[tokio::test]
    async fn help_title_reflects_tab() {
        let mut app = App::new_test();
        app.help_return_mode = AppMode::List;
        app.active_tab = ResourceType::Pod;
        assert_eq!(help_title(&app), "Help — Pods");
        app.active_tab = ResourceType::Deployment;
        assert_eq!(help_title(&app), "Help — Deployments");
    }
}
