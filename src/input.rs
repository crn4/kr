use crate::app::{App, LOG_CHROME_LINES};
use crate::models::{AppMode, KubeResource, KubeResourceEvent, PendingAction, ResourceType};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashSet;

pub fn handle_input(app: &mut App, key: KeyEvent) {
    match app.mode {
        AppMode::FilterInput => handle_filter_input(app, key),
        AppMode::SecretDecode => handle_secret_modal_input(app, key),
        AppMode::ContextSelect => handle_popup_input(app, key),
        AppMode::NamespaceSelect => handle_namespace_input(app, key),
        AppMode::LogView => handle_log_input(app, key),
        AppMode::LogSearchInput => handle_log_search_input(app, key),
        AppMode::ScaleInput => handle_scale_input(app, key),
        AppMode::Confirm => handle_confirm_input(app, key),
        AppMode::ShellView => handle_shell_input(app, key),
        AppMode::DescribeView => handle_describe_input(app, key),
        AppMode::StatusFilter => handle_status_filter_input(app, key),
        AppMode::List => handle_global_input(app, key),
    }
}

fn handle_popup_input(app: &mut App, key: KeyEvent) {
    let len = app.available_contexts.len();
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::List;
        }
        KeyCode::Enter => {
            if let Some(i) = app.popup_state.selected()
                && let Some(ctx) = app.available_contexts.get(i)
            {
                app.pending_context = Some(ctx.clone());
            }
            app.mode = AppMode::List;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let i = app
                .popup_state
                .selected()
                .map(|i| i.saturating_sub(1))
                .unwrap_or(0);
            app.popup_state.select(Some(i));
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let i = app
                .popup_state
                .selected()
                .map(|i| (i + 1).min(len.saturating_sub(1)))
                .unwrap_or(0);
            app.popup_state.select(Some(i));
        }
        _ => {}
    }
}

fn is_valid_k8s_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 63
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && s.starts_with(|c: char| c.is_ascii_alphanumeric())
        && s.ends_with(|c: char| c.is_ascii_alphanumeric())
}

fn select_namespace(app: &mut App, ns: String) {
    if !ns.is_empty() {
        app.current_namespace = ns.clone();
        let ctx = app.current_context.clone();
        app.app_state.add_namespace(&ctx, &ns);
        if !app.available_namespaces.contains(&ns) {
            app.available_namespaces.push(ns);
            app.available_namespaces.sort();
        }
        app.app_state.save();
    }
    app.namespace_input.clear();
    app.namespace_typing = false;
    app.mode = AppMode::List;
}

fn handle_namespace_input(app: &mut App, key: KeyEvent) {
    if app.namespace_typing {
        match key.code {
            KeyCode::Esc => {
                app.namespace_input.clear();
                app.namespace_typing = false;
                app.filtered_namespaces
                    .clone_from(&app.available_namespaces);
                let idx = app
                    .filtered_namespaces
                    .iter()
                    .position(|ns| *ns == app.current_namespace);
                app.popup_state.select(idx.or(Some(0)));
            }
            KeyCode::Enter => {
                let ns = app
                    .popup_state
                    .selected()
                    .and_then(|i| app.filtered_namespaces.get(i).cloned())
                    .unwrap_or_else(|| app.namespace_input.clone());
                if is_valid_k8s_name(&ns) {
                    select_namespace(app, ns);
                } else {
                    app.set_error("Invalid namespace name (RFC 1123: lowercase, digits, hyphens, max 63 chars)".to_string());
                }
            }
            KeyCode::Up => {
                let i = app
                    .popup_state
                    .selected()
                    .map(|i| i.saturating_sub(1))
                    .unwrap_or(0);
                app.popup_state.select(Some(i));
            }
            KeyCode::Down => {
                let len = app.filtered_namespaces.len();
                if len > 0 {
                    let i = app
                        .popup_state
                        .selected()
                        .map(|i| (i + 1).min(len.saturating_sub(1)))
                        .unwrap_or(0);
                    app.popup_state.select(Some(i));
                }
            }
            KeyCode::Backspace => {
                app.namespace_input.pop();
                app.update_namespace_filter();
            }
            KeyCode::Char(c) => {
                app.namespace_input.push(c);
                app.update_namespace_filter();
            }
            _ => {}
        }
    } else {
        let len = app.filtered_namespaces.len();
        match key.code {
            KeyCode::Esc => {
                app.namespace_input.clear();
                app.namespace_typing = false;
                app.mode = AppMode::List;
            }
            KeyCode::Char('/') => {
                app.namespace_typing = true;
                app.namespace_input.clear();
            }
            KeyCode::Enter => {
                if let Some(ns) = app
                    .popup_state
                    .selected()
                    .and_then(|i| app.filtered_namespaces.get(i).cloned())
                {
                    select_namespace(app, ns);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = app
                    .popup_state
                    .selected()
                    .map(|i| i.saturating_sub(1))
                    .unwrap_or(0);
                app.popup_state.select(Some(i));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if len > 0 {
                    let i = app
                        .popup_state
                        .selected()
                        .map(|i| (i + 1).min(len.saturating_sub(1)))
                        .unwrap_or(0);
                    app.popup_state.select(Some(i));
                }
            }
            _ => {}
        }
    }
}

fn log_max_scroll(app: &App) -> usize {
    let visible = crossterm::terminal::size()
        .map(|(_, h)| (h as usize).saturating_sub(LOG_CHROME_LINES))
        .unwrap_or(20);
    app.log_buffer.len().saturating_sub(visible)
}

fn handle_log_input(app: &mut App, key: KeyEvent) {
    let page_size = crossterm::terminal::size()
        .map(|(_, h)| (h as usize).saturating_sub(LOG_CHROME_LINES))
        .unwrap_or(20);

    match key.code {
        KeyCode::Char('q') => {
            app.abort_log_stream();
            app.mode = AppMode::List;
        }
        KeyCode::Esc => {
            if !app.log_search_query.is_empty() {
                app.log_search_query.clear();
                app.log_search_match_line = None;
                app.log_search_pending = false;
            } else {
                app.abort_log_stream();
                app.mode = AppMode::List;
            }
        }
        KeyCode::Char('/') => {
            app.log_search_input.clone_from(&app.log_search_query);
            app.mode = AppMode::LogSearchInput;
        }
        KeyCode::Char('n') => {
            app.log_search_next();
        }
        KeyCode::Char('N') => {
            app.log_search_prev();
        }
        KeyCode::Char('j') | KeyCode::Down => {
            let max = log_max_scroll(app);
            if let Some(offset) = &mut app.log_scroll_offset {
                if *offset < max {
                    *offset += 1;
                }
            } else if max > 0 {
                app.log_scroll_offset = Some(max);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(offset) = &mut app.log_scroll_offset {
                if *offset > 0 {
                    *offset -= 1;
                } else {
                    app.load_more_history();
                }
            } else {
                let max = log_max_scroll(app);
                if max > 0 {
                    app.log_scroll_offset = Some(max.saturating_sub(1));
                }
            }
        }
        KeyCode::PageDown => {
            let max = log_max_scroll(app);
            if let Some(offset) = &mut app.log_scroll_offset {
                *offset = (*offset + page_size).min(max);
            } else if max > 0 {
                app.log_scroll_offset = Some(max);
            }
        }
        KeyCode::PageUp => {
            if let Some(offset) = &mut app.log_scroll_offset {
                if *offset == 0 {
                    app.load_more_history();
                } else {
                    *offset = offset.saturating_sub(page_size);
                }
            } else {
                let max = log_max_scroll(app);
                if max > 0 {
                    app.log_scroll_offset = Some(max.saturating_sub(page_size));
                }
            }
        }
        KeyCode::Char('G') => {
            app.log_scroll_offset = None;
        }
        KeyCode::Char('g') => {
            app.log_scroll_offset = Some(0);
        }
        _ => {}
    }
}

fn handle_log_search_input(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            app.log_search_query = app.log_search_input.to_ascii_lowercase();
            app.log_search_match_line = None;
            app.mode = AppMode::LogView;
            app.log_search_next();
        }
        KeyCode::Esc => {
            app.log_search_input.clear();
            app.mode = AppMode::LogView;
        }
        KeyCode::Backspace => {
            app.log_search_input.pop();
        }
        KeyCode::Char(c) => {
            app.log_search_input.push(c);
        }
        _ => {}
    }
}

fn handle_global_input(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Tab => app.next_tab(),
        KeyCode::BackTab => app.prev_tab(),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        KeyCode::Char('c') => {
            let current_idx = app
                .available_contexts
                .iter()
                .position(|ctx| *ctx == app.current_context);
            app.popup_state.select(current_idx.or(Some(0)));
            app.mode = AppMode::ContextSelect;
        }
        KeyCode::Char('n') => {
            app.namespace_input.clear();
            app.namespace_typing = false;
            app.filtered_namespaces
                .clone_from(&app.available_namespaces);
            let current_idx = app
                .filtered_namespaces
                .iter()
                .position(|ns| *ns == app.current_namespace);
            app.popup_state
                .select(current_idx.or(if app.filtered_namespaces.is_empty() {
                    None
                } else {
                    Some(0)
                }));
            app.mode = AppMode::NamespaceSelect;
        }
        KeyCode::Char('/') => {
            app.mode = AppMode::FilterInput;
        }
        KeyCode::Char('j') | KeyCode::Down => next_row(app),
        KeyCode::Char('k') | KeyCode::Up => prev_row(app),
        KeyCode::Char('g') => {
            if !app.filtered_items.is_empty() {
                app.table_state.select(Some(0));
            }
        }
        KeyCode::Char('G') => {
            let len = app.filtered_items.len();
            if len > 0 {
                app.table_state.select(Some(len - 1));
            }
        }
        KeyCode::PageDown => {
            let len = app.filtered_items.len();
            if len > 0 {
                let page = crossterm::terminal::size()
                    .map(|(_, h)| (h as usize).saturating_sub(8))
                    .unwrap_or(20);
                let i = app.table_state.selected().unwrap_or(0);
                app.table_state.select(Some((i + page).min(len - 1)));
            }
        }
        KeyCode::PageUp => {
            if !app.filtered_items.is_empty() {
                let page = crossterm::terminal::size()
                    .map(|(_, h)| (h as usize).saturating_sub(8))
                    .unwrap_or(20);
                let i = app.table_state.selected().unwrap_or(0);
                app.table_state.select(Some(i.saturating_sub(page)));
            }
        }

        KeyCode::Char(' ') if app.active_tab != ResourceType::Secret => {
            if let Some(i) = app.table_state.selected()
                && !app.selected_indices.remove(&i)
            {
                app.selected_indices.insert(i);
            }
        }
        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if app.selected_indices.len() == app.filtered_items.len() {
                app.selected_indices.clear();
            } else {
                app.selected_indices = (0..app.filtered_items.len()).collect();
            }
        }

        KeyCode::Char('f') if app.active_tab == ResourceType::Pod => {
            app.build_status_filter_items();
            app.status_filter_state
                .select(if app.status_filter_items.is_empty() {
                    None
                } else {
                    Some(0)
                });
            app.mode = AppMode::StatusFilter;
        }

        KeyCode::Char('l') if app.active_tab == ResourceType::Pod => {
            if let Some(pod) = app.get_selected_resource() {
                let name = pod.name().to_owned();
                let ns = app.current_namespace.clone();
                app.stream_logs(&name, &ns);
            } else {
                app.set_error("No pod selected".to_string());
            }
        }
        KeyCode::Char('s') if app.active_tab == ResourceType::Pod => {
            if let Some(pod) = app.get_selected_resource() {
                let name = pod.name().to_owned();
                let ns = app.current_namespace.clone();
                app.start_shell(&name, &ns);
            } else {
                app.set_error("No pod selected".to_string());
            }
        }
        KeyCode::Delete | KeyCode::Char('D')
            if app.active_tab == ResourceType::Pod
                || app.active_tab == ResourceType::Deployment =>
        {
            let (count, names): (usize, Vec<String>) = if app.selected_indices.is_empty() {
                if let Some(r) = app.get_selected_resource() {
                    (1, vec![r.name().to_string()])
                } else {
                    (0, vec![])
                }
            } else {
                let mut indices: Vec<usize> = app.selected_indices.iter().copied().collect();
                indices.sort_unstable();
                let names: Vec<String> = indices
                    .iter()
                    .filter_map(|&i| app.filtered_items.get(i).map(|r| r.name().to_string()))
                    .collect();
                (names.len(), names)
            };
            if count > 0 {
                let kind = match app.active_tab {
                    ResourceType::Pod => "pod(s)",
                    ResourceType::Deployment => "deployment(s)",
                    _ => "resource(s)",
                };
                app.pending_action = Some(PendingAction::DeleteResource { count, kind, names });
                app.mode = AppMode::Confirm;
            } else {
                app.set_error("No resource selected".to_string());
            }
        }

        KeyCode::Char('S') if app.active_tab == ResourceType::Deployment => {
            if app.get_selected_resource().is_some() {
                app.scale_input.clear();
                app.mode = AppMode::ScaleInput;
            } else {
                app.set_error("No deployment selected".to_string());
            }
        }
        KeyCode::Char('r') if app.active_tab == ResourceType::Deployment => {
            if let Some(res) = app.get_selected_resource() {
                let name = res.name().to_string();
                app.pending_action = Some(PendingAction::RestartDeployment { name });
                app.mode = AppMode::Confirm;
            } else {
                app.set_error("No deployment selected".to_string());
            }
        }

        KeyCode::Char('d')
            if app.active_tab == ResourceType::Pod
                || app.active_tab == ResourceType::Deployment =>
        {
            if let Some(res) = app.get_selected_resource() {
                let kind = match app.active_tab {
                    ResourceType::Pod => "pod",
                    ResourceType::Deployment => "deployment",
                    _ => return,
                };
                let name = res.name().to_owned();
                let ns = app.current_namespace.clone();
                let ctx = app.current_context.clone();
                let tx = app.event_tx.clone();
                tokio::spawn(async move {
                    match tokio::process::Command::new("kubectl")
                        .args(["describe", kind, &name, "-n", &ns, "--context", &ctx])
                        .output()
                        .await
                    {
                        Ok(output) if output.status.success() => {
                            let text = String::from_utf8_lossy(&output.stdout);
                            let lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
                            let _ = tx.send(KubeResourceEvent::DescribeReady(lines));
                        }
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            let _ = tx.send(KubeResourceEvent::Error(format!(
                                "Describe failed: {}",
                                stderr.trim()
                            )));
                        }
                        Err(e) => {
                            let _ =
                                tx.send(KubeResourceEvent::Error(format!("Describe failed: {e}")));
                        }
                    }
                });
            } else {
                app.set_error("No resource selected".to_string());
            }
        }

        KeyCode::Char('e')
            if app.active_tab == ResourceType::Pod
                || app.active_tab == ResourceType::Deployment =>
        {
            if let Some(res) = app.get_selected_resource() {
                let kind = match app.active_tab {
                    ResourceType::Pod => "pod",
                    ResourceType::Deployment => "deployment",
                    _ => return,
                };
                let name = res.name().to_owned();
                let ns = app.current_namespace.clone();
                app.start_kubectl_edit(kind, &name, &ns);
            } else {
                app.set_error("No resource selected".to_string());
            }
        }

        KeyCode::Enter | KeyCode::Char('x') if app.active_tab == ResourceType::Secret => {
            app.decode_selected_secret();
            if app.selected_secret_decoded.is_some() {
                app.secret_scroll = 0;
                app.secret_revealed = false;
                app.mode = AppMode::SecretDecode;
            }
        }

        KeyCode::Esc => {
            app.filter_query.clear();
            app.status_filter.clear();
            app.update_filter();
        }
        _ => {}
    }
}

fn handle_filter_input(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::List;
        }
        KeyCode::Enter => {
            app.mode = AppMode::List;
        }
        KeyCode::Backspace => {
            app.filter_query.pop();
            app.update_filter();
        }
        KeyCode::Char(c) => {
            app.filter_query.push(c);
            app.update_filter();
        }
        _ => {}
    }
}

fn handle_secret_modal_input(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.mode = AppMode::List;
            app.selected_secret_decoded = None;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(decoded) = &app.selected_secret_decoded
                && app.secret_scroll < decoded.len().saturating_sub(1)
            {
                app.secret_scroll += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.secret_scroll = app.secret_scroll.saturating_sub(1);
        }
        KeyCode::Char('r') => {
            app.secret_revealed = !app.secret_revealed;
        }
        KeyCode::Char('c') => {
            if let Some(decoded) = &app.selected_secret_decoded
                && let Some((key, value)) = decoded.get(app.secret_scroll)
            {
                match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(value.clone())) {
                    Ok(()) => {
                        if let Some(handle) = app.clipboard_clear_task.take() {
                            handle.abort();
                        }
                        app.set_success(format!("Copied '{key}' to clipboard (clears in 15s)"));
                        let handle = tokio::spawn(async {
                            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                            if let Ok(mut cb) = arboard::Clipboard::new() {
                                let _ = cb.set_text(String::new());
                            }
                        });
                        app.clipboard_clear_task = Some(handle.abort_handle());
                    }
                    Err(e) => app.set_error(format!("Clipboard error: {e}")),
                }
            }
        }
        _ => {}
    }
}

fn describe_max_scroll(app: &App) -> usize {
    let visible = crossterm::terminal::size()
        .map(|(_, h)| ((h as usize) * 90 / 100).saturating_sub(2))
        .unwrap_or(20);
    app.describe_content.len().saturating_sub(visible)
}

fn handle_describe_input(app: &mut App, key: KeyEvent) {
    let page_size = crossterm::terminal::size()
        .map(|(_, h)| ((h as usize) * 90 / 100).saturating_sub(2))
        .unwrap_or(20);

    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.describe_content.clear();
            app.mode = AppMode::List;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            let max = describe_max_scroll(app);
            if app.describe_scroll < max {
                app.describe_scroll += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.describe_scroll = app.describe_scroll.saturating_sub(1);
        }
        KeyCode::PageDown => {
            let max = describe_max_scroll(app);
            app.describe_scroll = (app.describe_scroll + page_size).min(max);
        }
        KeyCode::PageUp => {
            app.describe_scroll = app.describe_scroll.saturating_sub(page_size);
        }
        KeyCode::Char('G') => {
            app.describe_scroll = describe_max_scroll(app);
        }
        KeyCode::Char('g') => {
            app.describe_scroll = 0;
        }
        _ => {}
    }
}

fn handle_status_filter_input(app: &mut App, key: KeyEvent) {
    let len = app.status_filter_items.len();
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::List;
        }
        KeyCode::Enter => {
            let selected = if app.status_filter_selected.is_empty() {
                app.status_filter_state
                    .selected()
                    .into_iter()
                    .collect::<HashSet<_>>()
            } else {
                app.status_filter_selected.clone()
            };
            if selected.len() == app.status_filter_items.len() {
                app.status_filter.clear();
            } else {
                app.status_filter = selected
                    .iter()
                    .filter_map(|&i| {
                        app.status_filter_items
                            .get(i)
                            .map(|(phase, _)| phase.clone())
                    })
                    .collect();
            }
            app.update_filter();
            app.mode = AppMode::List;
        }
        KeyCode::Char(' ') => {
            if let Some(i) = app.status_filter_state.selected()
                && !app.status_filter_selected.remove(&i)
            {
                app.status_filter_selected.insert(i);
            }
        }
        KeyCode::Char('a') => {
            if app.status_filter_selected.len() == len {
                app.status_filter_selected.clear();
            } else {
                app.status_filter_selected = (0..len).collect();
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let i = app
                .status_filter_state
                .selected()
                .map(|i| i.saturating_sub(1))
                .unwrap_or(0);
            app.status_filter_state.select(Some(i));
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if len > 0 {
                let i = app
                    .status_filter_state
                    .selected()
                    .map(|i| (i + 1).min(len.saturating_sub(1)))
                    .unwrap_or(0);
                app.status_filter_state.select(Some(i));
            }
        }
        _ => {}
    }
}

fn handle_shell_input(app: &mut App, key: KeyEvent) {
    use std::io::Write;

    if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.shell_session = None;
        app.mode = AppMode::List;
        return;
    }

    let bytes = key_to_pty_bytes(key);
    if !bytes.is_empty()
        && let Some(session) = &mut app.shell_session
    {
        let _ = session.writer.write_all(&bytes);
    }
}

fn key_to_pty_bytes(key: KeyEvent) -> Vec<u8> {
    let has_alt = key.modifiers.contains(KeyModifiers::ALT);

    if key.modifiers.contains(KeyModifiers::CONTROL)
        && let KeyCode::Char(c) = key.code
    {
        let code = (c as u8).wrapping_sub(b'a').wrapping_add(1);
        if has_alt {
            return vec![0x1b, code];
        }
        return vec![code];
    }

    if has_alt && let KeyCode::Char(c) = key.code {
        let mut bytes = vec![0x1b];
        let mut buf = [0u8; 4];
        c.encode_utf8(&mut buf);
        bytes.extend_from_slice(&buf[..c.len_utf8()]);
        return bytes;
    }

    match key.code {
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            c.encode_utf8(&mut buf);
            buf[..c.len_utf8()].to_vec()
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        _ => vec![],
    }
}

fn handle_scale_input(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::List;
        }
        KeyCode::Enter => {
            if app.scale_input.is_empty() {
                app.set_error("Enter a replica count".to_string());
                return;
            }
            if let Ok(replicas) = app.scale_input.parse::<u32>() {
                if replicas > 1000 {
                    app.set_error("Replica count must be <= 1000".to_string());
                } else if let Some(res) = app.get_selected_resource() {
                    let name = res.name().to_owned();
                    app.pending_action = Some(PendingAction::ScaleDeployment { name, replicas });
                    app.mode = AppMode::Confirm;
                    return;
                }
            } else {
                app.set_error("Invalid number".to_string());
            }
            app.mode = AppMode::List;
        }
        KeyCode::Backspace => {
            app.scale_input.pop();
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            app.scale_input.push(c);
        }
        _ => {}
    }
}

fn handle_confirm_input(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            if let Some(action) = app.pending_action.take() {
                match action {
                    PendingAction::DeleteResource { .. } => {
                        let indices: Vec<usize> = if app.selected_indices.is_empty() {
                            app.table_state.selected().into_iter().collect()
                        } else {
                            let mut v: Vec<usize> = app.selected_indices.iter().copied().collect();
                            v.sort_unstable();
                            v
                        };
                        for idx in indices {
                            if let Some(item) = app.filtered_items.get(idx).cloned() {
                                let client = app.client.clone();
                                let ns = app.current_namespace.clone();
                                let tx = app.event_tx.clone();
                                match item {
                                    KubeResource::Pod(p) => {
                                        let name = p.metadata.name.clone().unwrap_or_default();
                                        tokio::spawn(async move {
                                            let result =
                                                crate::k8s::actions::delete_pod(client, &ns, &name)
                                                    .await;
                                            let _ = tx.send(match result {
                                                Ok(()) => KubeResourceEvent::Success(format!(
                                                    "Pod '{name}' deleted"
                                                )),
                                                Err(e) => KubeResourceEvent::Error(format!(
                                                    "Delete '{name}' failed: {e}"
                                                )),
                                            });
                                        });
                                    }
                                    KubeResource::Deployment(d) => {
                                        let name = d.metadata.name.clone().unwrap_or_default();
                                        tokio::spawn(async move {
                                            let result = crate::k8s::actions::delete_deployment(
                                                client, &ns, &name,
                                            )
                                            .await;
                                            let _ = tx.send(match result {
                                                Ok(()) => KubeResourceEvent::Success(format!(
                                                    "Deployment '{name}' deleted"
                                                )),
                                                Err(e) => KubeResourceEvent::Error(format!(
                                                    "Delete '{name}' failed: {e}"
                                                )),
                                            });
                                        });
                                    }
                                    KubeResource::Secret(_) => {}
                                }
                            }
                        }
                    }
                    PendingAction::RestartDeployment { name } => {
                        let client = app.client.clone();
                        let ns = app.current_namespace.clone();
                        let tx = app.event_tx.clone();
                        tokio::spawn(async move {
                            let result =
                                crate::k8s::actions::rollout_restart(client, &ns, &name).await;
                            let _ = tx.send(match result {
                                Ok(()) => {
                                    KubeResourceEvent::Success(format!("Rollout restart: '{name}'"))
                                }
                                Err(e) => KubeResourceEvent::Error(format!(
                                    "Restart '{name}' failed: {e}"
                                )),
                            });
                        });
                    }
                    PendingAction::ScaleDeployment { name, replicas } => {
                        let client = app.client.clone();
                        let ns = app.current_namespace.clone();
                        let tx = app.event_tx.clone();
                        tokio::spawn(async move {
                            let result =
                                crate::k8s::actions::scale_deployment(client, &ns, &name, replicas)
                                    .await;
                            let _ = tx.send(match result {
                                Ok(()) => KubeResourceEvent::Success(format!(
                                    "'{name}' scaled to {replicas} replicas"
                                )),
                                Err(e) => {
                                    KubeResourceEvent::Error(format!("Scale '{name}' failed: {e}"))
                                }
                            });
                        });
                    }
                }
                app.selected_indices.clear();
            }
            app.mode = AppMode::List;
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.selected_indices.clear();
            app.pending_action = None;
            app.mode = AppMode::List;
        }
        _ => {}
    }
}

fn next_row(app: &mut App) {
    let len = app.filtered_items.len();
    if len == 0 {
        return;
    }
    let i = match app.table_state.selected() {
        Some(i) => (i + 1) % len,
        None => 0,
    };
    app.table_state.select(Some(i));
}

fn prev_row(app: &mut App) {
    let len = app.filtered_items.len();
    if len == 0 {
        return;
    }
    let i = match app.table_state.selected() {
        Some(i) => {
            if i == 0 {
                len - 1
            } else {
                i - 1
            }
        }
        None => len - 1,
    };
    app.table_state.select(Some(i));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::models::{AppMode, KubeResource, PendingAction, ResourceType};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use k8s_openapi::api::core::v1::Pod;
    use std::sync::Arc;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn key_with_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn make_pod(name: &str) -> KubeResource {
        let mut pod = Pod::default();
        pod.metadata.name = Some(name.to_string());
        KubeResource::Pod(Arc::new(pod))
    }

    #[tokio::test]
    async fn nav_j_moves_down() {
        let mut app = App::new_test();
        app.filtered_items = vec![make_pod("a"), make_pod("b"), make_pod("c")];

        handle_input(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.table_state.selected(), Some(0));

        handle_input(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.table_state.selected(), Some(1));
    }

    #[tokio::test]
    async fn nav_k_moves_up() {
        let mut app = App::new_test();
        app.filtered_items = vec![make_pod("a"), make_pod("b"), make_pod("c")];
        app.table_state.select(Some(2));

        handle_input(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.table_state.selected(), Some(1));
    }

    #[tokio::test]
    async fn nav_wraps_forward() {
        let mut app = App::new_test();
        app.filtered_items = vec![make_pod("a"), make_pod("b")];
        app.table_state.select(Some(1));

        handle_input(&mut app, key(KeyCode::Down));
        assert_eq!(app.table_state.selected(), Some(0));
    }

    #[tokio::test]
    async fn nav_wraps_backward() {
        let mut app = App::new_test();
        app.filtered_items = vec![make_pod("a"), make_pod("b")];
        app.table_state.select(Some(0));

        handle_input(&mut app, key(KeyCode::Up));
        assert_eq!(app.table_state.selected(), Some(1));
    }

    #[tokio::test]
    async fn nav_empty_list_does_nothing() {
        let mut app = App::new_test();
        handle_input(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.table_state.selected(), None);
    }

    #[tokio::test]
    async fn tab_switches_forward() {
        let mut app = App::new_test();
        assert_eq!(app.active_tab, ResourceType::Pod);

        handle_input(&mut app, key(KeyCode::Tab));
        assert_eq!(app.active_tab, ResourceType::Deployment);

        handle_input(&mut app, key(KeyCode::Tab));
        assert_eq!(app.active_tab, ResourceType::Secret);
    }

    #[tokio::test]
    async fn backtab_switches_backward() {
        let mut app = App::new_test();
        handle_input(&mut app, key(KeyCode::BackTab));
        assert_eq!(app.active_tab, ResourceType::Secret);
    }

    #[tokio::test]
    async fn q_quits() {
        let mut app = App::new_test();
        handle_input(&mut app, key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[tokio::test]
    async fn slash_enters_filter_mode() {
        let mut app = App::new_test();
        handle_input(&mut app, key(KeyCode::Char('/')));
        assert_eq!(app.mode, AppMode::FilterInput);
    }

    #[tokio::test]
    async fn ctrl_c_quits() {
        let mut app = App::new_test();
        handle_input(
            &mut app,
            key_with_mod(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(app.should_quit);
    }

    #[tokio::test]
    async fn c_opens_context_select() {
        let mut app = App::new_test();
        handle_input(&mut app, key(KeyCode::Char('c')));
        assert_eq!(app.mode, AppMode::ContextSelect);
        assert_eq!(app.popup_state.selected(), Some(0));
    }

    #[tokio::test]
    async fn n_opens_namespace_select() {
        let mut app = App::new_test();
        handle_input(&mut app, key(KeyCode::Char('n')));
        assert_eq!(app.mode, AppMode::NamespaceSelect);
        assert_eq!(app.popup_state.selected(), Some(0));
    }

    #[tokio::test]
    async fn filter_input_adds_chars() {
        let mut app = App::new_test();
        app.mode = AppMode::FilterInput;
        app.items = vec![make_pod("nginx"), make_pod("redis")];

        handle_input(&mut app, key(KeyCode::Char('n')));
        assert_eq!(app.filter_query, "n");

        handle_input(&mut app, key(KeyCode::Char('g')));
        assert_eq!(app.filter_query, "ng");
    }

    #[tokio::test]
    async fn filter_backspace_removes_char() {
        let mut app = App::new_test();
        app.mode = AppMode::FilterInput;
        app.filter_query = "abc".to_string();
        app.items = vec![make_pod("abc")];

        handle_input(&mut app, key(KeyCode::Backspace));
        assert_eq!(app.filter_query, "ab");
    }

    #[tokio::test]
    async fn filter_esc_returns_to_list() {
        let mut app = App::new_test();
        app.mode = AppMode::FilterInput;

        handle_input(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::List);
    }

    #[tokio::test]
    async fn filter_enter_returns_to_list() {
        let mut app = App::new_test();
        app.mode = AppMode::FilterInput;

        handle_input(&mut app, key(KeyCode::Enter));
        assert_eq!(app.mode, AppMode::List);
    }

    #[tokio::test]
    async fn popup_j_k_navigation() {
        let mut app = App::new_test();
        app.mode = AppMode::ContextSelect;
        app.available_contexts = vec!["a".into(), "b".into(), "c".into()];
        app.popup_state.select(Some(0));

        handle_input(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.popup_state.selected(), Some(1));

        handle_input(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.popup_state.selected(), Some(0));
    }

    #[tokio::test]
    async fn popup_enter_selects_namespace() {
        let mut app = App::new_test();
        app.mode = AppMode::NamespaceSelect;
        app.available_namespaces = vec!["default".into(), "staging".into()];
        app.filtered_namespaces = vec!["default".into(), "staging".into()];
        app.popup_state.select(Some(1));

        handle_input(&mut app, key(KeyCode::Enter));
        assert_eq!(app.current_namespace, "staging");
        assert_eq!(app.mode, AppMode::List);
    }

    #[tokio::test]
    async fn namespace_input_accepts_custom_text() {
        let mut app = App::new_test();
        app.mode = AppMode::NamespaceSelect;
        app.available_namespaces = vec![];
        app.filtered_namespaces = vec![];
        app.popup_state.select(None);

        handle_input(&mut app, key(KeyCode::Char('/')));
        assert!(app.namespace_typing);

        handle_input(&mut app, key(KeyCode::Char('m')));
        handle_input(&mut app, key(KeyCode::Char('y')));
        handle_input(&mut app, key(KeyCode::Char('-')));
        handle_input(&mut app, key(KeyCode::Char('n')));
        handle_input(&mut app, key(KeyCode::Char('s')));
        assert_eq!(app.namespace_input, "my-ns");

        handle_input(&mut app, key(KeyCode::Enter));
        assert_eq!(app.current_namespace, "my-ns");
        assert_eq!(app.mode, AppMode::List);
    }

    #[tokio::test]
    async fn namespace_input_filters_list() {
        let mut app = App::new_test();
        app.mode = AppMode::NamespaceSelect;
        app.available_namespaces = vec!["default".into(), "kube-system".into(), "dev".into()];
        app.filtered_namespaces = vec!["default".into(), "kube-system".into(), "dev".into()];

        handle_input(&mut app, key(KeyCode::Char('/')));
        handle_input(&mut app, key(KeyCode::Char('d')));
        handle_input(&mut app, key(KeyCode::Char('e')));
        assert_eq!(app.filtered_namespaces.len(), 2);
        assert_eq!(app.filtered_namespaces[0], "default");
        assert_eq!(app.filtered_namespaces[1], "dev");
    }

    #[tokio::test]
    async fn namespace_jk_scrolls_in_default_mode() {
        let mut app = App::new_test();
        app.mode = AppMode::NamespaceSelect;
        app.available_namespaces = vec!["a".into(), "b".into(), "c".into()];
        app.filtered_namespaces = vec!["a".into(), "b".into(), "c".into()];
        app.popup_state.select(Some(0));

        handle_input(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.popup_state.selected(), Some(1));

        handle_input(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.popup_state.selected(), Some(0));
    }

    #[tokio::test]
    async fn namespace_esc_typing_returns_to_scroll() {
        let mut app = App::new_test();
        app.mode = AppMode::NamespaceSelect;
        app.available_namespaces = vec!["default".into()];
        app.filtered_namespaces = vec!["default".into()];
        app.popup_state.select(Some(0));

        handle_input(&mut app, key(KeyCode::Char('/')));
        assert!(app.namespace_typing);

        handle_input(&mut app, key(KeyCode::Esc));
        assert!(!app.namespace_typing);
        assert_eq!(app.mode, AppMode::NamespaceSelect);
    }

    #[tokio::test]
    async fn popup_enter_selects_context() {
        let mut app = App::new_test();
        app.mode = AppMode::ContextSelect;
        app.available_contexts = vec!["dev".into(), "prod".into()];
        app.popup_state.select(Some(1));

        handle_input(&mut app, key(KeyCode::Enter));
        assert_eq!(app.pending_context, Some("prod".to_string()));
        assert_eq!(app.mode, AppMode::List);
    }

    #[tokio::test]
    async fn popup_esc_cancels() {
        let mut app = App::new_test();
        app.mode = AppMode::ContextSelect;

        handle_input(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::List);
        assert!(app.pending_context.is_none());
    }

    #[tokio::test]
    async fn secret_enter_opens_decode() {
        let mut app = App::new_test();
        app.active_tab = ResourceType::Secret;
        let mut secret = k8s_openapi::api::core::v1::Secret::default();
        secret.metadata.name = Some("s1".to_string());
        secret.data = Some(std::collections::BTreeMap::new());
        app.filtered_items = vec![KubeResource::Secret(Arc::new(secret))];
        app.table_state.select(Some(0));

        handle_input(&mut app, key(KeyCode::Enter));
        assert_eq!(app.mode, AppMode::SecretDecode);
        assert!(app.selected_secret_decoded.is_some());
    }

    #[tokio::test]
    async fn secret_modal_esc_closes() {
        let mut app = App::new_test();
        app.mode = AppMode::SecretDecode;
        app.selected_secret_decoded = Some(vec![("k".into(), "v".into())]);

        handle_input(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::List);
        assert!(app.selected_secret_decoded.is_none());
    }

    #[tokio::test]
    async fn secret_modal_scroll() {
        let mut app = App::new_test();
        app.mode = AppMode::SecretDecode;
        app.selected_secret_decoded = Some(vec![
            ("a".into(), "1".into()),
            ("b".into(), "2".into()),
            ("c".into(), "3".into()),
        ]);
        app.secret_scroll = 0;

        handle_input(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.secret_scroll, 1);

        handle_input(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.secret_scroll, 2);

        handle_input(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.secret_scroll, 2);

        handle_input(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.secret_scroll, 1);
    }

    #[tokio::test]
    async fn log_esc_exits_to_list() {
        let mut app = App::new_test();
        app.mode = AppMode::LogView;

        handle_input(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::List);
    }

    #[tokio::test]
    async fn log_q_exits_to_list() {
        let mut app = App::new_test();
        app.mode = AppMode::LogView;

        handle_input(&mut app, key(KeyCode::Char('q')));
        assert_eq!(app.mode, AppMode::List);
    }

    #[tokio::test]
    async fn log_scroll_up_at_top_triggers_load_more() {
        let mut app = App::new_test();
        app.mode = AppMode::LogView;
        app.log_pod_name = "test-pod".into();
        app.log_namespace = "default".into();
        for i in 0..50 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_scroll_offset = Some(0);

        handle_input(&mut app, key(KeyCode::Char('k')));
        assert!(app.log_loading_history);
        assert_eq!(app.log_tail_lines, 200);
    }

    #[tokio::test]
    async fn log_load_more_skips_when_already_loading() {
        let mut app = App::new_test();
        app.mode = AppMode::LogView;
        app.log_pod_name = "test-pod".into();
        app.log_namespace = "default".into();
        app.log_loading_history = true;
        app.log_tail_lines = 200;
        app.log_scroll_offset = Some(0);

        handle_input(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.log_tail_lines, 200);
    }

    #[tokio::test]
    async fn log_pageup_at_top_triggers_load_more() {
        let mut app = App::new_test();
        app.mode = AppMode::LogView;
        app.log_pod_name = "test-pod".into();
        app.log_namespace = "default".into();
        for i in 0..50 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_scroll_offset = Some(0);

        handle_input(&mut app, key(KeyCode::PageUp));
        assert!(app.log_loading_history);
        assert_eq!(app.log_tail_lines, 200);
    }

    #[tokio::test]
    async fn scale_accepts_digits() {
        let mut app = App::new_test();
        app.mode = AppMode::ScaleInput;

        handle_input(&mut app, key(KeyCode::Char('3')));
        assert_eq!(app.scale_input, "3");

        handle_input(&mut app, key(KeyCode::Char('5')));
        assert_eq!(app.scale_input, "35");
    }

    #[tokio::test]
    async fn scale_rejects_letters() {
        let mut app = App::new_test();
        app.mode = AppMode::ScaleInput;

        handle_input(&mut app, key(KeyCode::Char('a')));
        assert_eq!(app.scale_input, "");
    }

    #[tokio::test]
    async fn scale_backspace() {
        let mut app = App::new_test();
        app.mode = AppMode::ScaleInput;
        app.scale_input = "12".to_string();

        handle_input(&mut app, key(KeyCode::Backspace));
        assert_eq!(app.scale_input, "1");
    }

    #[tokio::test]
    async fn scale_esc_cancels() {
        let mut app = App::new_test();
        app.mode = AppMode::ScaleInput;

        handle_input(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::List);
    }

    #[tokio::test]
    async fn confirm_n_cancels() {
        let mut app = App::new_test();
        app.mode = AppMode::Confirm;
        app.pending_action = Some(PendingAction::DeleteResource {
            count: 1,
            kind: "pod(s)",
            names: vec!["test".into()],
        });

        handle_input(&mut app, key(KeyCode::Char('n')));
        assert_eq!(app.mode, AppMode::List);
        assert!(app.pending_action.is_none());
    }

    #[tokio::test]
    async fn confirm_esc_cancels() {
        let mut app = App::new_test();
        app.mode = AppMode::Confirm;
        app.pending_action = Some(PendingAction::DeleteResource {
            count: 1,
            kind: "pod(s)",
            names: vec!["test".into()],
        });

        handle_input(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::List);
        assert!(app.pending_action.is_none());
    }

    #[tokio::test]
    async fn delete_key_opens_confirm_for_pod() {
        let mut app = App::new_test();
        app.active_tab = ResourceType::Pod;
        app.filtered_items = vec![make_pod("nginx")];
        app.table_state.select(Some(0));

        handle_input(&mut app, key(KeyCode::Delete));
        assert_eq!(app.mode, AppMode::Confirm);
        assert!(app.pending_action.is_some());
    }

    #[tokio::test]
    async fn s_starts_shell_for_pod() {
        let mut app = App::new_test();
        app.active_tab = ResourceType::Pod;
        app.filtered_items = vec![make_pod("nginx")];
        app.table_state.select(Some(0));

        handle_input(&mut app, key(KeyCode::Char('s')));
        assert!(app.mode == AppMode::ShellView || app.last_error.is_some());
    }

    #[tokio::test]
    async fn shift_s_opens_scale_for_deployment() {
        let mut app = App::new_test();
        app.active_tab = ResourceType::Deployment;
        let mut dep = k8s_openapi::api::apps::v1::Deployment::default();
        dep.metadata.name = Some("web".to_string());
        app.filtered_items = vec![KubeResource::Deployment(Arc::new(dep))];
        app.table_state.select(Some(0));

        handle_input(&mut app, key(KeyCode::Char('S')));
        assert_eq!(app.mode, AppMode::ScaleInput);
    }

    #[tokio::test]
    async fn namespace_rejects_invalid_name() {
        let mut app = App::new_test();
        app.mode = AppMode::NamespaceSelect;
        app.available_namespaces = vec![];
        app.filtered_namespaces = vec![];
        app.popup_state.select(None);

        handle_input(&mut app, key(KeyCode::Char('/')));
        handle_input(&mut app, key(KeyCode::Char('M')));
        handle_input(&mut app, key(KeyCode::Char('y')));
        handle_input(&mut app, key(KeyCode::Enter));
        assert!(app.last_error.is_some());
    }

    #[tokio::test]
    async fn namespace_rejects_trailing_hyphen() {
        let mut app = App::new_test();
        app.mode = AppMode::NamespaceSelect;
        app.available_namespaces = vec![];
        app.filtered_namespaces = vec![];
        app.popup_state.select(None);

        handle_input(&mut app, key(KeyCode::Char('/')));
        for c in "my-ns-".chars() {
            handle_input(&mut app, key(KeyCode::Char(c)));
        }
        handle_input(&mut app, key(KeyCode::Enter));
        assert!(app.last_error.is_some());
    }

    #[tokio::test]
    async fn scale_rejects_over_1000() {
        let mut app = App::new_test();
        app.mode = AppMode::ScaleInput;
        app.active_tab = ResourceType::Deployment;
        let mut dep = k8s_openapi::api::apps::v1::Deployment::default();
        dep.metadata.name = Some("web".to_string());
        app.filtered_items = vec![KubeResource::Deployment(Arc::new(dep))];
        app.table_state.select(Some(0));
        app.scale_input = "9999".to_string();

        handle_input(&mut app, key(KeyCode::Enter));
        assert!(app.last_error.is_some());
        assert!(app.last_error.as_ref().unwrap().contains("1000"));
    }

    #[tokio::test]
    async fn esc_in_list_clears_filter() {
        let mut app = App::new_test();
        app.items = vec![make_pod("a"), make_pod("b")];
        app.filter_query = "a".to_string();
        app.status_filter.insert("Running".to_string());
        app.update_filter();

        handle_input(&mut app, key(KeyCode::Esc));
        assert_eq!(app.filter_query, "");
        assert!(app.status_filter.is_empty());
        assert_eq!(app.filtered_items.len(), 2);
    }

    #[test]
    fn pty_alt_char_sends_esc_prefix() {
        let ev = key_with_mod(KeyCode::Char(':'), KeyModifiers::ALT);
        assert_eq!(key_to_pty_bytes(ev), vec![0x1b, b':']);
    }

    #[test]
    fn pty_alt_letter_sends_esc_prefix() {
        let ev = key_with_mod(KeyCode::Char('d'), KeyModifiers::ALT);
        assert_eq!(key_to_pty_bytes(ev), vec![0x1b, b'd']);
    }

    #[test]
    fn pty_ctrl_alt_sends_esc_plus_control_code() {
        let ev = key_with_mod(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        );
        assert_eq!(key_to_pty_bytes(ev), vec![0x1b, 0x03]);
    }

    #[test]
    fn pty_plain_char_no_esc_prefix() {
        let ev = key(KeyCode::Char(':'));
        assert_eq!(key_to_pty_bytes(ev), vec![b':']);
    }

    fn make_pod_with_status(name: &str, phase: &str) -> KubeResource {
        use k8s_openapi::api::core::v1::PodStatus;
        let mut pod = Pod::default();
        pod.metadata.name = Some(name.to_string());
        pod.status = Some(PodStatus {
            phase: Some(phase.to_string()),
            ..Default::default()
        });
        KubeResource::Pod(Arc::new(pod))
    }

    #[tokio::test]
    async fn f_opens_status_filter() {
        let mut app = App::new_test();
        app.items = vec![
            make_pod_with_status("a", "Running"),
            make_pod_with_status("b", "Pending"),
        ];
        handle_input(&mut app, key(KeyCode::Char('f')));
        assert_eq!(app.mode, AppMode::StatusFilter);
        assert_eq!(app.status_filter_items.len(), 2);
    }

    #[tokio::test]
    async fn f_ignored_on_deployment_tab() {
        let mut app = App::new_test();
        app.active_tab = ResourceType::Deployment;
        handle_input(&mut app, key(KeyCode::Char('f')));
        assert_eq!(app.mode, AppMode::List);
    }

    #[tokio::test]
    async fn status_filter_space_toggles() {
        let mut app = App::new_test();
        app.items = vec![
            make_pod_with_status("a", "Running"),
            make_pod_with_status("b", "Pending"),
        ];
        app.build_status_filter_items();
        app.status_filter_state.select(Some(0));
        app.mode = AppMode::StatusFilter;

        handle_input(&mut app, key(KeyCode::Char(' ')));
        assert!(app.status_filter_selected.contains(&0));

        handle_input(&mut app, key(KeyCode::Char(' ')));
        assert!(!app.status_filter_selected.contains(&0));
    }

    #[tokio::test]
    async fn status_filter_enter_applies() {
        let mut app = App::new_test();
        app.items = vec![
            make_pod_with_status("a", "Running"),
            make_pod_with_status("b", "Pending"),
            make_pod_with_status("c", "Running"),
        ];
        app.update_filter();
        assert_eq!(app.filtered_items.len(), 3);

        app.build_status_filter_items();
        app.status_filter_state.select(Some(0));
        app.mode = AppMode::StatusFilter;

        let running_idx = app
            .status_filter_items
            .iter()
            .position(|(p, _)| p == "Pending")
            .unwrap();
        app.status_filter_selected.insert(running_idx);

        handle_input(&mut app, key(KeyCode::Enter));
        assert_eq!(app.mode, AppMode::List);
        assert_eq!(app.filtered_items.len(), 1);
        assert_eq!(app.filtered_items[0].name(), "b");
    }

    #[tokio::test]
    async fn status_filter_esc_cancels() {
        let mut app = App::new_test();
        app.items = vec![
            make_pod_with_status("a", "Running"),
            make_pod_with_status("b", "Pending"),
        ];
        app.update_filter();
        app.build_status_filter_items();
        app.status_filter_state.select(Some(0));
        app.status_filter_selected.insert(0);
        app.mode = AppMode::StatusFilter;

        handle_input(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::List);
        assert!(app.status_filter.is_empty());
        assert_eq!(app.filtered_items.len(), 2);
    }

    #[tokio::test]
    async fn status_filter_a_toggles_all() {
        let mut app = App::new_test();
        app.items = vec![
            make_pod_with_status("a", "Running"),
            make_pod_with_status("b", "Pending"),
        ];
        app.build_status_filter_items();
        app.status_filter_state.select(Some(0));
        app.mode = AppMode::StatusFilter;

        handle_input(&mut app, key(KeyCode::Char('a')));
        assert_eq!(app.status_filter_selected.len(), 2);

        handle_input(&mut app, key(KeyCode::Char('a')));
        assert!(app.status_filter_selected.is_empty());
    }

    #[tokio::test]
    async fn status_filter_enter_selects_cursor_when_none_toggled() {
        let mut app = App::new_test();
        app.items = vec![
            make_pod_with_status("a", "Running"),
            make_pod_with_status("b", "Pending"),
            make_pod_with_status("c", "Running"),
        ];
        app.update_filter();
        app.build_status_filter_items();

        let pending_idx = app
            .status_filter_items
            .iter()
            .position(|(p, _)| p == "Pending")
            .unwrap();
        app.status_filter_state.select(Some(pending_idx));
        app.mode = AppMode::StatusFilter;

        handle_input(&mut app, key(KeyCode::Enter));
        assert_eq!(app.mode, AppMode::List);
        assert_eq!(app.filtered_items.len(), 1);
        assert_eq!(app.filtered_items[0].name(), "b");
    }

    #[tokio::test]
    async fn log_slash_enters_search_input() {
        let mut app = App::new_test();
        app.mode = AppMode::LogView;

        handle_input(&mut app, key(KeyCode::Char('/')));
        assert_eq!(app.mode, AppMode::LogSearchInput);
    }

    #[tokio::test]
    async fn log_search_input_accumulates_chars() {
        let mut app = App::new_test();
        app.mode = AppMode::LogSearchInput;

        handle_input(&mut app, key(KeyCode::Char('e')));
        handle_input(&mut app, key(KeyCode::Char('r')));
        handle_input(&mut app, key(KeyCode::Char('r')));
        assert_eq!(app.log_search_input, "err");
    }

    #[tokio::test]
    async fn log_search_enter_confirms() {
        let mut app = App::new_test();
        app.mode = AppMode::LogSearchInput;
        app.log_search_input = "test".to_string();

        handle_input(&mut app, key(KeyCode::Enter));
        assert_eq!(app.mode, AppMode::LogView);
        assert_eq!(app.log_search_query, "test");
    }

    #[tokio::test]
    async fn log_search_esc_cancels() {
        let mut app = App::new_test();
        app.mode = AppMode::LogSearchInput;
        app.log_search_input = "test".to_string();
        app.log_search_query = "old".to_string();

        handle_input(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::LogView);
        assert_eq!(app.log_search_input, "");
        assert_eq!(app.log_search_query, "old");
    }

    #[tokio::test]
    async fn log_esc_clears_search_first() {
        let mut app = App::new_test();
        app.mode = AppMode::LogView;
        app.log_search_query = "test".to_string();

        handle_input(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::LogView);
        assert_eq!(app.log_search_query, "");
    }

    #[tokio::test]
    async fn log_esc_exits_when_no_search() {
        let mut app = App::new_test();
        app.mode = AppMode::LogView;
        app.log_search_query.clear();

        handle_input(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::List);
    }

    #[tokio::test]
    async fn log_n_jumps_to_next_match() {
        let mut app = App::new_test();
        app.mode = AppMode::LogView;
        for i in 0..50 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_buffer.push_back("error found here".to_string());
        for i in 51..200 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_scroll_offset = Some(100);
        app.log_search_query = "error".to_string();

        handle_input(&mut app, key(KeyCode::Char('n')));
        assert_eq!(app.log_search_match_line, Some(50));
    }

    #[tokio::test]
    async fn log_n_finds_above_scroll() {
        let mut app = App::new_test();
        app.mode = AppMode::LogView;
        app.log_buffer.push_back("error first".to_string());
        for i in 1..50 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_scroll_offset = Some(10);
        app.log_search_query = "error".to_string();

        handle_input(&mut app, key(KeyCode::Char('n')));
        assert_eq!(app.log_search_match_line, Some(0));
    }

    #[tokio::test]
    async fn log_shift_n_jumps_to_prev() {
        let mut app = App::new_test();
        app.mode = AppMode::LogView;
        for i in 0..80 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_buffer.push_back("error found here".to_string());
        for i in 81..200 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_scroll_offset = Some(50);
        app.log_search_query = "error".to_string();

        handle_input(&mut app, key(KeyCode::Char('N')));
        assert_eq!(app.log_search_match_line, Some(80));
    }
}
