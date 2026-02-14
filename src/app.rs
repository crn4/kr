use crate::models::{AppMode, KubeResource, KubeResourceEvent, PendingAction, ResourceType};
use crate::state::AppState;
use k8s_openapi::api::{
    apps::v1::Deployment,
    core::v1::{Pod, Secret},
};
use kube::runtime::reflector::Store;
use kube::Client;
use ratatui::widgets::{ListState, TableState};
use std::collections::{HashSet, VecDeque};
use std::io::Read;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::AbortHandle;

pub struct ShellSession {
    pub writer: Box<dyn std::io::Write + Send>,
    pub parser: vt100::Parser,
    _master: Box<dyn portable_pty::MasterPty + Send>,
}

const MAX_LOG_LINES: usize = 10_000;

pub struct App {
    pub client: Client,
    pub current_namespace: String,

    pub mode: AppMode,
    pub active_tab: ResourceType,
    pub should_quit: bool,

    // Stores
    pub pod_store: Option<Store<Pod>>,
    pub deployment_store: Option<Store<Deployment>>,
    pub secret_store: Option<Store<Secret>>,

    // Contexts
    pub current_context: String,
    pub pending_context: Option<String>,

    // Internal event sender
    pub event_tx: UnboundedSender<KubeResourceEvent>,

    pub items: Vec<KubeResource>,
    pub filtered_items: Vec<KubeResource>,
    pub table_state: TableState,
    pub filter_query: String,
    pub selected_indices: HashSet<usize>,

    pub selected_secret_decoded: Option<Vec<(String, String)>>,
    pub log_buffer: VecDeque<String>,
    pub log_task: Option<AbortHandle>,
    pub log_scroll_offset: Option<usize>, // None = auto-follow

    pub available_contexts: Vec<String>,
    pub available_namespaces: Vec<String>,
    pub filtered_namespaces: Vec<String>,
    pub namespace_input: String,
    pub namespace_typing: bool,
    pub popup_state: ListState,

    pub last_error: Option<String>,
    pub last_success: Option<String>,
    pub message_time: Option<Instant>,
    pub is_loading: bool,
    pub loading_since: Option<Instant>,
    pub dirty: bool,

    // Secret modal scroll
    pub secret_scroll: usize,
    pub secret_table_state: TableState,
    pub secret_revealed: bool,

    // Scale input
    pub scale_input: String,

    // Confirmation dialog
    pub pending_action: Option<PendingAction>,

    // Describe view
    pub describe_content: Vec<String>,
    pub describe_scroll: usize,

    // Embedded shell session
    pub shell_session: Option<ShellSession>,

    // Clipboard auto-clear task
    pub clipboard_clear_task: Option<AbortHandle>,

    // Persistent state
    pub app_state: AppState,
}

impl App {
    pub async fn new(client: Client) -> anyhow::Result<(Self, tokio::sync::mpsc::UnboundedReceiver<KubeResourceEvent>)> {
        let namespace = crate::k8s::config::get_context_namespace()
            .unwrap_or_else(|_| "default".to_string());
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        Ok((Self {
            client,
            current_namespace: namespace,
            mode: AppMode::List,
            active_tab: ResourceType::Pod,
            should_quit: false,
            pod_store: None,
            deployment_store: None,
            secret_store: None,
            event_tx: tx,
            items: Vec::new(),
            filtered_items: Vec::new(),
            table_state: TableState::default(),
            filter_query: String::new(),
            selected_indices: HashSet::new(),
            selected_secret_decoded: None,
            log_buffer: VecDeque::new(),
            log_task: None,
            log_scroll_offset: None,
            current_context: "default".into(), // Will be updated
            pending_context: None,
            available_contexts: Vec::new(),
            available_namespaces: Vec::new(),
            filtered_namespaces: Vec::new(),
            namespace_input: String::new(),
            namespace_typing: false,
            popup_state: ListState::default(),
            last_error: None,
            last_success: None,
            message_time: None,
            is_loading: true,
            loading_since: Some(Instant::now()),
            dirty: true,
            secret_scroll: 0,
            secret_table_state: TableState::default(),
            secret_revealed: false,
            scale_input: String::new(),
            pending_action: None,
            describe_content: Vec::new(),
            describe_scroll: 0,
            shell_session: None,
            clipboard_clear_task: None,
            app_state: AppState::load(),
        }, rx))
    }

    pub fn next_tab(&mut self) {
        self.active_tab = match self.active_tab {
            ResourceType::Pod => ResourceType::Deployment,
            ResourceType::Deployment => ResourceType::Secret,
            ResourceType::Secret => ResourceType::Pod,
        };
        self.reset_tab_state();
    }

    pub fn prev_tab(&mut self) {
        self.active_tab = match self.active_tab {
            ResourceType::Pod => ResourceType::Secret,
            ResourceType::Deployment => ResourceType::Pod,
            ResourceType::Secret => ResourceType::Deployment,
        };
        self.reset_tab_state();
    }

    fn reset_tab_state(&mut self) {
        self.items.clear();
        self.filtered_items.clear();
        self.table_state.select(None);
        self.selected_indices.clear();
    }

    pub fn get_selected_resource(&self) -> Option<&KubeResource> {
        self.table_state
            .selected()
            .and_then(|i| self.filtered_items.get(i))
    }

    pub fn decode_selected_secret(&mut self) {
        if let Some(KubeResource::Secret(s)) = self.get_selected_resource().cloned() {
            if let Some(data) = &s.data {
                let decoded: Vec<(String, String)> = data
                    .iter()
                    .map(|(k, v)| {
                        let val = String::from_utf8(v.0.clone())
                            .unwrap_or_else(|_| "<binary>".to_string());
                        (k.clone(), val)
                    })
                    .collect();
                self.selected_secret_decoded = Some(decoded);
            } else {
                self.selected_secret_decoded = Some(vec![]);
            }
        }
    }

    pub fn stream_logs(&mut self, pod_name: &str, namespace: &str) {
        self.abort_log_stream();
        self.log_buffer.clear();
        self.log_scroll_offset = None;
        self.mode = AppMode::LogView;

        let abort = crate::k8s::actions::stream_pod_logs(
            self.client.clone(),
            namespace,
            pod_name,
            self.event_tx.clone(),
        );
        self.log_task = Some(abort);
    }

    pub fn abort_log_stream(&mut self) {
        if let Some(handle) = self.log_task.take() {
            handle.abort();
        }
    }

    pub fn load_namespaces(&self) {
        let client = self.client.clone();
        let current_ns = self.current_namespace.clone();
        let ctx = self.current_context.clone();
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            // Try kube-rs API first
            use k8s_openapi::api::core::v1::Namespace;
            use kube::api::ListParams;
            use kube::Api;
            let ns_api: Api<Namespace> = Api::all(client);
            if let Ok(ns_list) = ns_api.list(&ListParams::default()).await {
                let namespaces: Vec<String> = ns_list
                    .iter()
                    .map(|n| n.metadata.name.clone().unwrap_or_default())
                    .collect();
                let _ = tx.send(KubeResourceEvent::NamespacesLoaded(namespaces));
                return;
            }

            // Fallback: try kubectl CLI (handles Teleport/exec auth plugins)
            if let Ok(output) = tokio::process::Command::new("kubectl")
                .args([
                    "get", "namespaces", "--context", &ctx,
                    "-o", "jsonpath={.items[*].metadata.name}",
                ])
                .output()
                .await
            {
                if output.status.success() {
                    let text = String::from_utf8_lossy(&output.stdout);
                    let namespaces: Vec<String> = text
                        .split_whitespace()
                        .map(|s| s.to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !namespaces.is_empty() {
                        let _ = tx.send(KubeResourceEvent::NamespacesLoaded(namespaces));
                        return;
                    }
                }
            }

            // Last resort: just the current namespace
            let _ = tx.send(KubeResourceEvent::NamespacesLoaded(vec![current_ns]));
        });
    }

    pub fn update_namespace_filter(&mut self) {
        if self.namespace_input.is_empty() {
            self.filtered_namespaces.clone_from(&self.available_namespaces);
        } else {
            let query = self.namespace_input.to_lowercase();
            self.filtered_namespaces = self
                .available_namespaces
                .iter()
                .filter(|ns| ns.to_lowercase().contains(&query))
                .cloned()
                .collect();
        }
        // Reset selection
        if self.filtered_namespaces.is_empty() {
            self.popup_state.select(None);
        } else {
            self.popup_state.select(Some(0));
        }
    }

    pub fn set_error(&mut self, msg: String) {
        self.last_error = Some(msg);
        self.last_success = None;
        self.message_time = Some(Instant::now());
    }

    pub fn set_success(&mut self, msg: String) {
        self.last_success = Some(msg);
        self.last_error = None;
        self.message_time = Some(Instant::now());
    }

    pub fn clear_stale_messages(&mut self) {
        if let Some(t) = self.message_time {
            let elapsed = t.elapsed().as_secs();
            // Success messages clear after 5s
            if self.last_success.is_some() && elapsed >= 5 {
                self.last_success = None;
                if self.last_error.is_none() {
                    self.message_time = None;
                }
            }
            // Errors persist for 15s (access denied errors persist until tab/context switch)
            if let Some(err) = &self.last_error {
                if !err.starts_with("Access denied") && elapsed >= 15 {
                    self.last_error = None;
                    self.message_time = None;
                }
            }
        }
    }

    pub fn start_shell(&mut self, pod_name: &str, namespace: &str) {
        use portable_pty::CommandBuilder;
        let mut cmd = CommandBuilder::new("kubectl");
        cmd.args([
            "exec", "-it", pod_name, "-n", namespace, "--context",
            &self.current_context, "--", "sh",
        ]);
        self.spawn_pty_session(cmd);
    }

    pub fn start_kubectl_edit(&mut self, kind: &str, name: &str, namespace: &str) {
        use portable_pty::CommandBuilder;
        let mut cmd = CommandBuilder::new("kubectl");
        cmd.args(["edit", kind, name, "-n", namespace, "--context", &self.current_context]);
        self.spawn_pty_session(cmd);
    }

    fn spawn_pty_session(&mut self, cmd: portable_pty::CommandBuilder) {
        use portable_pty::{native_pty_system, PtySize};

        let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
        let pty_rows = (rows * 80 / 100).saturating_sub(2).max(10);
        let pty_cols = (cols * 80 / 100).saturating_sub(2).max(40);

        let pty_system = native_pty_system();
        let pair = match pty_system.openpty(PtySize {
            rows: pty_rows,
            cols: pty_cols,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(pair) => pair,
            Err(e) => {
                self.set_error(format!("Failed to open PTY: {e}"));
                return;
            }
        };

        match pair.slave.spawn_command(cmd) {
            Ok(_child) => {}
            Err(e) => {
                self.set_error(format!("Failed to spawn command: {e}"));
                return;
            }
        }
        drop(pair.slave);

        let reader = match pair.master.try_clone_reader() {
            Ok(r) => r,
            Err(e) => {
                self.set_error(format!("Failed to get PTY reader: {e}"));
                return;
            }
        };

        let writer = match pair.master.take_writer() {
            Ok(w) => w,
            Err(e) => {
                self.set_error(format!("Failed to get PTY writer: {e}"));
                return;
            }
        };

        let parser = vt100::Parser::new(pty_rows, pty_cols, 0);

        let tx = self.event_tx.clone();
        tokio::task::spawn_blocking(move || {
            let mut reader = reader;
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => {
                        let _ = tx.send(KubeResourceEvent::ShellExited);
                        break;
                    }
                    Ok(n) => {
                        if tx.send(KubeResourceEvent::ShellOutput(buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        self.shell_session = Some(ShellSession {
            writer,
            parser,
            _master: pair.master,
        });
        self.mode = AppMode::ShellView;
    }

    pub fn push_log_line(&mut self, line: String) {
        if self.log_buffer.len() >= MAX_LOG_LINES {
            self.log_buffer.pop_front();
        }
        self.log_buffer.push_back(line);
    }

    pub fn refresh_items(&mut self) {
        self.items.clear();
        match self.active_tab {
            ResourceType::Pod => {
                if let Some(store) = &self.pod_store {
                    self.items = store
                        .state()
                        .iter()
                        .map(|p| KubeResource::Pod(Arc::clone(p)))
                        .collect();
                }
            }
            ResourceType::Deployment => {
                if let Some(store) = &self.deployment_store {
                    self.items = store
                        .state()
                        .iter()
                        .map(|d| KubeResource::Deployment(Arc::clone(d)))
                        .collect();
                }
            }
            ResourceType::Secret => {
                if let Some(store) = &self.secret_store {
                    self.items = store
                        .state()
                        .iter()
                        .map(|s| KubeResource::Secret(Arc::clone(s)))
                        .collect();
                }
            }
        }
        self.items.sort_by(|a, b| a.name().cmp(b.name()));
        self.update_filter();
    }

    #[cfg(test)]
    pub fn new_test() -> Self {
        use bytes::Bytes;
        use tower::ServiceBuilder;

        // Create a mock kube::Client that returns empty response
        let mock_service = tower::service_fn(|_req: http::Request<kube::client::Body>| async {
            Ok::<_, std::convert::Infallible>(http::Response::builder()
                .status(200)
                .body(kube::client::Body::from(Bytes::from_static(b"{\"kind\":\"PodList\",\"apiVersion\":\"v1\",\"metadata\":{},\"items\":[]}")))
                .unwrap())
        });
        let client = Client::new(ServiceBuilder::new().service(mock_service), "default");
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

        Self {
            client,
            current_namespace: "default".to_string(),
            mode: AppMode::List,
            active_tab: ResourceType::Pod,
            should_quit: false,
            pod_store: None,
            deployment_store: None,
            secret_store: None,
            event_tx: tx,
            items: Vec::new(),
            filtered_items: Vec::new(),
            table_state: TableState::default(),
            filter_query: String::new(),
            selected_indices: HashSet::new(),
            selected_secret_decoded: None,
            log_buffer: VecDeque::new(),
            log_task: None,
            log_scroll_offset: None,
            current_context: "test-context".into(),
            pending_context: None,
            available_contexts: vec!["ctx1".into(), "ctx2".into()],
            available_namespaces: vec!["default".into(), "kube-system".into()],
            filtered_namespaces: vec!["default".into(), "kube-system".into()],
            namespace_input: String::new(),
            namespace_typing: false,
            popup_state: ListState::default(),
            last_error: None,
            last_success: None,
            message_time: None,
            is_loading: false,
            loading_since: None,
            dirty: true,
            secret_scroll: 0,
            secret_table_state: TableState::default(),
            secret_revealed: false,
            scale_input: String::new(),
            pending_action: None,
            describe_content: Vec::new(),
            describe_scroll: 0,
            shell_session: None,
            clipboard_clear_task: None,
            app_state: AppState::default(),
        }
    }

    pub fn update_filter(&mut self) {
        self.selected_indices.clear();
        if self.filter_query.is_empty() {
            self.filtered_items.clone_from(&self.items);
        } else {
            let query = self.filter_query.to_lowercase();
            self.filtered_items = self
                .items
                .iter()
                .filter(|item| item.name().to_lowercase().contains(&query))
                .cloned()
                .collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::KubeResource;
    use k8s_openapi::api::core::v1::{Pod, Secret};
    use k8s_openapi::ByteString;
    use std::collections::BTreeMap;

    fn make_pod(name: &str) -> KubeResource {
        let mut pod = Pod::default();
        pod.metadata.name = Some(name.to_string());
        KubeResource::Pod(Arc::new(pod))
    }

    fn make_secret(name: &str, data: Vec<(&str, &str)>) -> KubeResource {
        let mut secret = Secret::default();
        secret.metadata.name = Some(name.to_string());
        let mut map = BTreeMap::new();
        for (k, v) in data {
            map.insert(k.to_string(), ByteString(v.as_bytes().to_vec()));
        }
        secret.data = Some(map);
        KubeResource::Secret(Arc::new(secret))
    }

    // --- Tab cycling ---

    #[tokio::test]
    async fn next_tab_cycles_forward() {
        let mut app = App::new_test();
        assert_eq!(app.active_tab, ResourceType::Pod);
        app.next_tab();
        assert_eq!(app.active_tab, ResourceType::Deployment);
        app.next_tab();
        assert_eq!(app.active_tab, ResourceType::Secret);
        app.next_tab();
        assert_eq!(app.active_tab, ResourceType::Pod);
    }

    #[tokio::test]
    async fn prev_tab_cycles_backward() {
        let mut app = App::new_test();
        assert_eq!(app.active_tab, ResourceType::Pod);
        app.prev_tab();
        assert_eq!(app.active_tab, ResourceType::Secret);
        app.prev_tab();
        assert_eq!(app.active_tab, ResourceType::Deployment);
        app.prev_tab();
        assert_eq!(app.active_tab, ResourceType::Pod);
    }

    #[tokio::test]
    async fn tab_switch_clears_state() {
        let mut app = App::new_test();
        app.items = vec![make_pod("a")];
        app.filtered_items = vec![make_pod("a")];
        app.table_state.select(Some(0));

        app.next_tab();

        assert!(app.items.is_empty());
        assert!(app.filtered_items.is_empty());
        assert_eq!(app.table_state.selected(), None);
    }

    // --- Filter ---

    #[tokio::test]
    async fn filter_empty_returns_all_items() {
        let mut app = App::new_test();
        app.items = vec![make_pod("nginx"), make_pod("redis"), make_pod("postgres")];
        app.filter_query.clear();
        app.update_filter();

        assert_eq!(app.filtered_items.len(), 3);
    }

    #[tokio::test]
    async fn filter_matches_substring() {
        let mut app = App::new_test();
        app.items = vec![make_pod("nginx"), make_pod("redis"), make_pod("nginx-proxy")];
        app.filter_query = "nginx".to_string();
        app.update_filter();

        assert_eq!(app.filtered_items.len(), 2);
        assert_eq!(app.filtered_items[0].name(), "nginx");
        assert_eq!(app.filtered_items[1].name(), "nginx-proxy");
    }

    #[tokio::test]
    async fn filter_case_insensitive() {
        let mut app = App::new_test();
        app.items = vec![make_pod("Nginx"), make_pod("REDIS")];
        app.filter_query = "nginx".to_string();
        app.update_filter();

        assert_eq!(app.filtered_items.len(), 1);
        assert_eq!(app.filtered_items[0].name(), "Nginx");
    }

    #[tokio::test]
    async fn filter_no_matches_returns_empty() {
        let mut app = App::new_test();
        app.items = vec![make_pod("nginx"), make_pod("redis")];
        app.filter_query = "postgres".to_string();
        app.update_filter();

        assert!(app.filtered_items.is_empty());
    }

    // --- Log buffer ---

    #[tokio::test]
    async fn push_log_line_appends() {
        let mut app = App::new_test();
        app.push_log_line("line1".to_string());
        app.push_log_line("line2".to_string());

        assert_eq!(app.log_buffer.len(), 2);
        assert_eq!(app.log_buffer[0], "line1");
        assert_eq!(app.log_buffer[1], "line2");
    }

    #[tokio::test]
    async fn push_log_line_respects_max_limit() {
        let mut app = App::new_test();
        for i in 0..MAX_LOG_LINES + 100 {
            app.push_log_line(format!("line{}", i));
        }

        assert_eq!(app.log_buffer.len(), MAX_LOG_LINES);
        // First line should be line100 (first 100 were evicted)
        assert_eq!(app.log_buffer[0], "line100");
    }

    // --- Selection ---

    #[tokio::test]
    async fn get_selected_resource_returns_none_when_no_selection() {
        let app = App::new_test();
        assert!(app.get_selected_resource().is_none());
    }

    #[tokio::test]
    async fn get_selected_resource_returns_correct_item() {
        let mut app = App::new_test();
        app.filtered_items = vec![make_pod("a"), make_pod("b"), make_pod("c")];
        app.table_state.select(Some(1));

        let res = app.get_selected_resource().unwrap();
        assert_eq!(res.name(), "b");
    }

    #[tokio::test]
    async fn get_selected_resource_out_of_bounds() {
        let mut app = App::new_test();
        app.filtered_items = vec![make_pod("a")];
        app.table_state.select(Some(5)); // out of bounds

        assert!(app.get_selected_resource().is_none());
    }

    // --- Secret decode ---

    #[tokio::test]
    async fn decode_selected_secret_extracts_data() {
        let mut app = App::new_test();
        app.active_tab = ResourceType::Secret;
        app.filtered_items = vec![make_secret("my-secret", vec![("user", "admin"), ("pass", "s3cret")])];
        app.table_state.select(Some(0));

        app.decode_selected_secret();

        let decoded = app.selected_secret_decoded.unwrap();
        assert_eq!(decoded.len(), 2);
        // BTreeMap is sorted by key
        assert!(decoded.iter().any(|(k, v)| k == "user" && v == "admin"));
        assert!(decoded.iter().any(|(k, v)| k == "pass" && v == "s3cret"));
    }

    #[tokio::test]
    async fn decode_selected_secret_empty_data() {
        let mut app = App::new_test();
        app.active_tab = ResourceType::Secret;
        let mut secret = Secret::default();
        secret.metadata.name = Some("empty".to_string());
        secret.data = None;
        app.filtered_items = vec![KubeResource::Secret(Arc::new(secret))];
        app.table_state.select(Some(0));

        app.decode_selected_secret();

        let decoded = app.selected_secret_decoded.unwrap();
        assert!(decoded.is_empty());
    }

    #[tokio::test]
    async fn decode_when_pod_selected_does_nothing() {
        let mut app = App::new_test();
        app.filtered_items = vec![make_pod("nginx")];
        app.table_state.select(Some(0));

        app.decode_selected_secret();

        assert!(app.selected_secret_decoded.is_none());
    }

    // --- Abort log stream ---

    #[tokio::test]
    async fn abort_log_stream_clears_handle() {
        let mut app = App::new_test();
        // No task — should not panic
        app.abort_log_stream();
        assert!(app.log_task.is_none());
    }

    // --- Dirty flag ---

    #[tokio::test]
    async fn new_app_starts_dirty() {
        let app = App::new_test();
        assert!(app.dirty);
    }
}
