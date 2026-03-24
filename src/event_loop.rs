use anyhow::Result;
use crossterm::event::{Event, EventStream};
use futures::{FutureExt, StreamExt};
use futures::stream::{BoxStream, SelectAll};
use ratatui::{Terminal, backend::Backend};
use std::time::Duration;
use tokio::time;

use crate::app::App;
use crate::input::handle_input;
use crate::k8s::watcher::reflect_resources;
use crate::models::{AppMode, KubeResourceEvent, ResourceType, TaggedWatcherEvent};
use crate::ui::draw;
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

fn tag_stream(
    stream: impl futures::Stream<Item = KubeResourceEvent> + Send + 'static,
    tab: ResourceType,
) -> BoxStream<'static, TaggedWatcherEvent> {
    Box::pin(stream.scan(false, move |stopped, event| {
        if *stopped {
            return std::future::ready(None);
        }
        if matches!(&event, KubeResourceEvent::WatcherForbidden(_)) {
            *stopped = true;
        }
        std::future::ready(Some(TaggedWatcherEvent { tab, event }))
    }))
}

fn create_watcher_for_tab(
    app: &mut App,
    tab: ResourceType,
) -> BoxStream<'static, TaggedWatcherEvent> {
    let client = app.client.clone();
    let ns = app.current_namespace.clone();

    match tab {
        ResourceType::Pod => {
            let (store, stream) = reflect_resources(client, &ns);
            app.pod_store = Some(store);
            tag_stream(stream.map(map_watcher_event), tab)
        }
        ResourceType::Deployment => {
            let (store, stream) = reflect_resources(client, &ns);
            app.deployment_store = Some(store);
            tag_stream(stream.map(map_watcher_event), tab)
        }
        ResourceType::Secret => {
            let (store, stream) = reflect_resources(client, &ns);
            app.secret_store = Some(store);
            tag_stream(stream.map(map_watcher_event), tab)
        }
    }
}

fn ensure_watcher(
    app: &mut App,
    tab: ResourceType,
    watchers: &mut SelectAll<BoxStream<'static, TaggedWatcherEvent>>,
    watcher_active: &mut [bool; 3],
) {
    let idx = tab.index();
    if watcher_active[idx] || app.tab_forbidden[idx] {
        return;
    }
    let stream = create_watcher_for_tab(app, tab);
    watchers.push(stream);
    watcher_active[idx] = true;
    app.tab_loading[idx] = true;
    app.tab_loading_since[idx] = Some(std::time::Instant::now());
}

fn handle_tagged_event(app: &mut App, tagged: TaggedWatcherEvent, watcher_active: &mut [bool; 3]) -> bool {
    let tab = tagged.tab;
    let idx = tab.index();
    let is_active = tab == app.active_tab;

    match tagged.event {
        KubeResourceEvent::WatcherForbidden(msg) => {
            watcher_active[idx] = false;
            app.tab_loading[idx] = false;
            app.tab_loading_since[idx] = None;
            app.tab_forbidden[idx] = true;
            if is_active {
                let resource_kind = match tab {
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
                app.dirty = true;
            }
            false
        }
        KubeResourceEvent::Error(msg) => {
            if is_active {
                app.set_error(msg);
                app.dirty = true;
            }
            false
        }
        KubeResourceEvent::InitialListDone => {
            app.tab_loading[idx] = false;
            app.tab_loading_since[idx] = None;
            if is_active {
                app.dirty = true;
                return true;
            }
            false
        }
        _ => is_active && !app.tab_loading[idx],
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
            app.describe_hscroll = 0;
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
    let mut watchers: SelectAll<BoxStream<'static, TaggedWatcherEvent>> = SelectAll::new();
    let mut watcher_active = [false; 3];
    ensure_watcher(&mut app, current_tab, &mut watchers, &mut watcher_active);

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

        let ns_or_ctx_changed = app.current_namespace != current_ns
            || app.current_context != current_ctx;

        if ns_or_ctx_changed {
            current_ns = app.current_namespace.clone();
            current_ctx = app.current_context.clone();
            current_tab = app.active_tab;

            watchers = SelectAll::new();
            watcher_active = [false; 3];
            app.items.clear();
            app.filtered_items.clear();
            app.pod_store = None;
            app.deployment_store = None;
            app.secret_store = None;
            app.tab_loading = [false; 3];
            app.tab_loading_since = [None; 3];
            app.tab_forbidden = [false; 3];
            if app
                .last_error
                .as_ref()
                .is_some_and(|e| e.starts_with("Access denied"))
            {
                app.last_error = None;
                app.message_time = None;
            }

            ensure_watcher(&mut app, current_tab, &mut watchers, &mut watcher_active);
            app.refresh_items();
            app.dirty = true;
        } else if app.active_tab != current_tab {
            current_tab = app.active_tab;

            ensure_watcher(&mut app, current_tab, &mut watchers, &mut watcher_active);
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
            Some(event) = watchers.next() => {
                let mut needs_refresh = handle_tagged_event(&mut app, event, &mut watcher_active);
                while let Some(Some(event)) = watchers.next().now_or_never() {
                    needs_refresh |= handle_tagged_event(&mut app, event, &mut watcher_active);
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

    use crate::app::App;

    #[tokio::test]
    async fn tagged_init_done_active_tab_returns_refresh() {
        let mut app = App::new_test();
        app.active_tab = ResourceType::Pod;
        app.tab_loading[0] = true;
        let mut watcher_active = [true, false, false];

        let tagged = TaggedWatcherEvent {
            tab: ResourceType::Pod,
            event: KubeResourceEvent::InitialListDone,
        };
        let needs_refresh = handle_tagged_event(&mut app, tagged, &mut watcher_active);

        assert!(needs_refresh);
        assert!(!app.tab_loading[0]);
        assert!(app.tab_loading_since[0].is_none());
    }

    #[tokio::test]
    async fn tagged_init_done_background_tab_no_refresh() {
        let mut app = App::new_test();
        app.active_tab = ResourceType::Pod;
        app.tab_loading[1] = true;
        let mut watcher_active = [true, true, false];

        let tagged = TaggedWatcherEvent {
            tab: ResourceType::Deployment,
            event: KubeResourceEvent::InitialListDone,
        };
        let needs_refresh = handle_tagged_event(&mut app, tagged, &mut watcher_active);

        assert!(!needs_refresh);
        assert!(!app.tab_loading[1]);
    }

    #[tokio::test]
    async fn tagged_forbidden_sets_tab_forbidden() {
        let mut app = App::new_test();
        app.active_tab = ResourceType::Pod;
        app.tab_loading[2] = true;
        let mut watcher_active = [true, false, true];

        let tagged = TaggedWatcherEvent {
            tab: ResourceType::Secret,
            event: KubeResourceEvent::WatcherForbidden("denied".into()),
        };
        handle_tagged_event(&mut app, tagged, &mut watcher_active);

        assert!(app.tab_forbidden[2]);
        assert!(!watcher_active[2]);
        assert!(!app.tab_loading[2]);
    }

    #[tokio::test]
    async fn tagged_forbidden_active_tab_shows_error() {
        let mut app = App::new_test();
        app.active_tab = ResourceType::Secret;
        let mut watcher_active = [false, false, true];

        let tagged = TaggedWatcherEvent {
            tab: ResourceType::Secret,
            event: KubeResourceEvent::WatcherForbidden("denied".into()),
        };
        handle_tagged_event(&mut app, tagged, &mut watcher_active);

        assert!(app.last_error.is_some());
        assert!(app.last_error.as_ref().unwrap().contains("Access denied"));
    }

    #[tokio::test]
    async fn tagged_forbidden_background_tab_no_error() {
        let mut app = App::new_test();
        app.active_tab = ResourceType::Pod;
        let mut watcher_active = [true, false, true];

        let tagged = TaggedWatcherEvent {
            tab: ResourceType::Secret,
            event: KubeResourceEvent::WatcherForbidden("denied".into()),
        };
        handle_tagged_event(&mut app, tagged, &mut watcher_active);

        assert!(app.tab_forbidden[2]);
        assert!(app.last_error.is_none());
    }

    #[tokio::test]
    async fn tagged_refresh_active_tab_needs_refresh() {
        let mut app = App::new_test();
        app.active_tab = ResourceType::Pod;
        let mut watcher_active = [true, false, false];

        let tagged = TaggedWatcherEvent {
            tab: ResourceType::Pod,
            event: KubeResourceEvent::Refresh,
        };
        let needs_refresh = handle_tagged_event(&mut app, tagged, &mut watcher_active);

        assert!(needs_refresh);
    }

    #[tokio::test]
    async fn tagged_refresh_background_tab_no_refresh() {
        let mut app = App::new_test();
        app.active_tab = ResourceType::Pod;
        let mut watcher_active = [true, true, false];

        let tagged = TaggedWatcherEvent {
            tab: ResourceType::Deployment,
            event: KubeResourceEvent::Refresh,
        };
        let needs_refresh = handle_tagged_event(&mut app, tagged, &mut watcher_active);

        assert!(!needs_refresh);
    }

    #[tokio::test]
    async fn tag_stream_terminates_after_forbidden() {
        let events = vec![
            KubeResourceEvent::Refresh,
            KubeResourceEvent::WatcherForbidden("denied".into()),
            KubeResourceEvent::Refresh,
        ];
        let stream = futures::stream::iter(events);
        let mut tagged = tag_stream(stream, ResourceType::Pod);

        let first = tagged.next().await;
        assert!(matches!(first, Some(TaggedWatcherEvent { event: KubeResourceEvent::Refresh, .. })));

        let second = tagged.next().await;
        assert!(matches!(second, Some(TaggedWatcherEvent { event: KubeResourceEvent::WatcherForbidden(_), .. })));

        let third = tagged.next().await;
        assert!(third.is_none());
    }

}
