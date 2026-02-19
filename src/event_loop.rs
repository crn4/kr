use anyhow::Result;
use crossterm::event::{Event, EventStream};
use futures::{FutureExt, StreamExt};
use ratatui::{Terminal, backend::Backend};
use std::time::Duration;
use tokio::time;

use crate::app::App;
use crate::input::handle_input;
use crate::k8s::watcher::reflect_resources;
use crate::models::{AppMode, KubeResourceEvent, ResourceType};
use crate::ui::draw;
use futures::stream::BoxStream;
use kube::runtime::watcher;

fn is_forbidden(err: &watcher::Error) -> bool {
    match err {
        watcher::Error::InitialListFailed(e)
        | watcher::Error::WatchStartFailed(e)
        | watcher::Error::WatchFailed(e) => {
            matches!(e, kube::Error::Api(resp) if resp.is_forbidden())
        }
        watcher::Error::WatchError(resp) => resp.is_forbidden(),
        _ => false,
    }
}

fn map_watcher_event<K>(event: Result<watcher::Event<K>, watcher::Error>) -> KubeResourceEvent {
    match event {
        Ok(watcher::Event::InitDone) => KubeResourceEvent::InitialListDone,
        Ok(_) => KubeResourceEvent::Refresh,
        Err(ref e) if is_forbidden(e) => {
            let msg = match e {
                watcher::Error::InitialListFailed(kube::Error::Api(resp))
                | watcher::Error::WatchStartFailed(kube::Error::Api(resp))
                | watcher::Error::WatchFailed(kube::Error::Api(resp)) => resp.message.clone(),
                watcher::Error::WatchError(resp) => resp.message.clone(),
                _ => String::new(),
            };
            KubeResourceEvent::WatcherForbidden(msg)
        }
        Err(e) => KubeResourceEvent::Error(format!("Watcher error: {e}")),
    }
}

fn create_watcher(app: &mut App) -> BoxStream<'static, KubeResourceEvent> {
    let client = app.client.clone();
    let ns = app.current_namespace.clone();

    match app.active_tab {
        ResourceType::Pod => {
            let (store, stream) = reflect_resources(client, &ns);
            app.pod_store = Some(store);
            Box::pin(stream.map(map_watcher_event))
        }
        ResourceType::Deployment => {
            let (store, stream) = reflect_resources(client, &ns);
            app.deployment_store = Some(store);
            Box::pin(stream.map(map_watcher_event))
        }
        ResourceType::Secret => {
            let (store, stream) = reflect_resources(client, &ns);
            app.secret_store = Some(store);
            Box::pin(stream.map(map_watcher_event))
        }
    }
}

fn handle_watcher_event(
    app: &mut App,
    event: KubeResourceEvent,
    watcher: &mut BoxStream<'static, KubeResourceEvent>,
) -> bool {
    match event {
        KubeResourceEvent::WatcherForbidden(msg) => {
            let resource_kind = match app.active_tab {
                ResourceType::Pod => "pods",
                ResourceType::Deployment => "deployments",
                ResourceType::Secret => "secrets",
            };
            let short_msg = if msg.is_empty() {
                format!("Access denied: cannot list {resource_kind}")
            } else {
                format!("Access denied: {resource_kind} — {msg}")
            };
            app.set_error(short_msg);
            app.is_loading = false;
            app.loading_since = None;
            *watcher = Box::pin(futures::stream::pending());
            app.dirty = true;
            false
        }
        KubeResourceEvent::Error(msg) => {
            app.set_error(msg);
            app.dirty = true;
            false
        }
        KubeResourceEvent::InitialListDone => {
            app.refresh_items();
            app.is_loading = false;
            app.loading_since = None;
            app.dirty = true;
            false
        }
        _ => !app.is_loading,
    }
}

fn handle_channel_event(app: &mut App, event: KubeResourceEvent) {
    match event {
        KubeResourceEvent::Refresh
        | KubeResourceEvent::InitialListDone
        | KubeResourceEvent::WatcherForbidden(_) => {}
        KubeResourceEvent::Log(line) => {
            app.push_log_line(line);
        }
        KubeResourceEvent::LogHistory(generation, lines) => {
            app.merge_log_history(generation, lines);
        }
        KubeResourceEvent::Error(e) => {
            app.set_error(e);
        }
        KubeResourceEvent::Success(msg) => {
            app.set_success(msg);
        }
        KubeResourceEvent::ShellOutput(data) => {
            if let Some(session) = &mut app.shell_session {
                session.parser.process(&data);
            }
        }
        KubeResourceEvent::ShellExited => {
            app.shell_session = None;
            if app.mode == AppMode::ShellView {
                app.mode = AppMode::List;
                app.set_success("Shell session ended".to_string());
            }
        }
        KubeResourceEvent::DescribeReady(lines) => {
            app.describe_content = lines;
            app.describe_scroll = 0;
            app.mode = AppMode::DescribeView;
        }
        KubeResourceEvent::NamespacesLoaded(namespaces) => {
            let ctx = app.current_context.clone();
            app.available_namespaces = app.app_state.merge_namespaces(&ctx, &namespaces);
            app.app_state.save();
        }
    }
    app.dirty = true;
}

pub async fn run<B: Backend<Error: Send + Sync + 'static> + std::io::Write>(
    terminal: &mut Terminal<B>,
    mut app: App,
    mut event_rx: tokio::sync::mpsc::UnboundedReceiver<KubeResourceEvent>,
) -> Result<()> {
    let mut reader = EventStream::new();
    let mut ticker = time::interval(Duration::from_millis(250));

    let mut current_tab = app.active_tab;
    let mut current_ns = app.current_namespace.clone();
    let mut watcher = create_watcher(&mut app);

    if let Ok(ctxs) = crate::k8s::config::list_contexts() {
        app.available_contexts = ctxs;
    }
    if let Ok(ctx) = crate::k8s::config::get_current_context() {
        app.current_context = ctx;
    }

    app.available_namespaces = app.app_state.get_namespaces(&app.current_context);
    if !app.available_namespaces.contains(&app.current_namespace) {
        app.available_namespaces.push(app.current_namespace.clone());
        app.available_namespaces.sort();
    }

    app.refresh_items();
    app.load_namespaces();

    let mut current_ctx = app.current_context.clone();

    loop {
        if app.dirty {
            terminal.draw(|f| draw(f, &mut app))?;
            app.dirty = false;
        }

        if app.should_quit {
            app.abort_log_stream();
            return Ok(());
        }

        if let Some(new_ctx) = app.pending_context.take() {
            crossterm::terminal::disable_raw_mode()?;
            crossterm::execute!(
                terminal.backend_mut(),
                crossterm::terminal::LeaveAlternateScreen,
                crossterm::cursor::Show
            )?;
            eprintln!("Authenticating with context '{new_ctx}'...");

            let result = crate::k8s::config::create_client_with_context(&new_ctx).await;

            crossterm::execute!(
                terminal.backend_mut(),
                crossterm::terminal::EnterAlternateScreen,
                crossterm::cursor::Hide
            )?;
            crossterm::terminal::enable_raw_mode()?;
            terminal.clear()?;

            match result {
                Ok(client) => {
                    app.client = client;
                    app.current_namespace = crate::k8s::config::get_namespace_for_context(&new_ctx);
                    app.current_context = new_ctx.clone();

                    app.available_namespaces = app.app_state.get_namespaces(&new_ctx);
                    if !app.available_namespaces.contains(&app.current_namespace) {
                        app.available_namespaces.push(app.current_namespace.clone());
                        app.available_namespaces.sort();
                    }
                    app.load_namespaces();
                }
                Err(e) => {
                    app.set_error(format!("Context switch failed: {e}"));
                }
            }
            app.dirty = true;
        }

        if app.active_tab != current_tab
            || app.current_namespace != current_ns
            || app.current_context != current_ctx
        {
            current_tab = app.active_tab;
            current_ns = app.current_namespace.clone();
            current_ctx = app.current_context.clone();

            app.items.clear();
            app.filtered_items.clear();
            app.pod_store = None;
            app.deployment_store = None;
            app.secret_store = None;
            app.is_loading = true;
            app.loading_since = Some(std::time::Instant::now());
            if app
                .last_error
                .as_ref()
                .is_some_and(|e| e.starts_with("Access denied"))
            {
                app.last_error = None;
                app.message_time = None;
            }

            watcher = create_watcher(&mut app);
            app.refresh_items();
            app.dirty = true;
        }

        tokio::select! {
            _ = ticker.tick() => {
                app.clear_stale_messages();
                app.dirty = true;
            }
            Some(Ok(event)) = reader.next() => {
               if let Event::Key(key) = event {
                   handle_input(&mut app, key);
                   app.dirty = true;
               }
            }
            Some(event) = watcher.next() => {
                let mut needs_refresh = handle_watcher_event(&mut app, event, &mut watcher);
                while let Some(Some(event)) = watcher.next().now_or_never() {
                    needs_refresh |= handle_watcher_event(&mut app, event, &mut watcher);
                }
                if needs_refresh {
                    app.refresh_items();
                    app.dirty = true;
                }
            }
            Some(event) = event_rx.recv() => {
                handle_channel_event(&mut app, event);
                while let Ok(event) = event_rx.try_recv() {
                    handle_channel_event(&mut app, event);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::Pod;
    use kube::core::Status;

    fn make_403_status() -> Box<Status> {
        Box::new(Status {
            message: "secrets is forbidden: User \"test\" cannot list resource \"secrets\""
                .to_string(),
            reason: "Forbidden".to_string(),
            code: 403,
            ..Default::default()
        })
    }

    fn make_404_status() -> Box<Status> {
        Box::new(Status {
            message: "not found".to_string(),
            reason: "NotFound".to_string(),
            code: 404,
            ..Default::default()
        })
    }

    #[test]
    fn is_forbidden_detects_initial_list_403() {
        let err = watcher::Error::InitialListFailed(kube::Error::Api(make_403_status()));
        assert!(is_forbidden(&err));
    }

    #[test]
    fn is_forbidden_detects_watch_start_403() {
        let err = watcher::Error::WatchStartFailed(kube::Error::Api(make_403_status()));
        assert!(is_forbidden(&err));
    }

    #[test]
    fn is_forbidden_detects_watch_error_403() {
        let err = watcher::Error::WatchError(make_403_status());
        assert!(is_forbidden(&err));
    }

    #[test]
    fn is_forbidden_detects_watch_failed_403() {
        let err = watcher::Error::WatchFailed(kube::Error::Api(make_403_status()));
        assert!(is_forbidden(&err));
    }

    #[test]
    fn is_forbidden_ignores_404() {
        let err = watcher::Error::InitialListFailed(kube::Error::Api(make_404_status()));
        assert!(!is_forbidden(&err));
    }

    #[test]
    fn is_forbidden_ignores_no_resource_version() {
        let err = watcher::Error::NoResourceVersion;
        assert!(!is_forbidden(&err));
    }

    #[test]
    fn map_watcher_event_403_returns_forbidden() {
        let err = watcher::Error::InitialListFailed(kube::Error::Api(make_403_status()));
        let event = map_watcher_event::<Pod>(Err(err));
        assert!(
            matches!(event, KubeResourceEvent::WatcherForbidden(msg) if msg.contains("forbidden"))
        );
    }

    #[test]
    fn map_watcher_event_404_returns_error() {
        let err = watcher::Error::InitialListFailed(kube::Error::Api(make_404_status()));
        let event = map_watcher_event::<Pod>(Err(err));
        assert!(matches!(event, KubeResourceEvent::Error(_)));
    }

    #[test]
    fn map_watcher_event_init_done_returns_initial_list_done() {
        let event = map_watcher_event::<Pod>(Ok(watcher::Event::InitDone));
        assert!(matches!(event, KubeResourceEvent::InitialListDone));
    }

    #[test]
    fn map_watcher_event_apply_returns_refresh() {
        let pod = Pod::default();
        let event = map_watcher_event::<Pod>(Ok(watcher::Event::Apply(pod)));
        assert!(matches!(event, KubeResourceEvent::Refresh));
    }
}
