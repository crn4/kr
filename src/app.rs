use crate::models::{AppMode, KubeResource, KubeResourceEvent, PendingAction, ResourceType};
use crate::state::AppState;
use k8s_openapi::api::{
    apps::v1::Deployment,
    core::v1::{Pod, Secret},
};
use kube::Client;
use kube::runtime::reflector::Store;
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

pub(crate) const MAX_LOG_LINES: usize = 10_000;
pub(crate) const LOG_CHROME_LINES: usize = 6;

pub(crate) fn contains_ascii_ci(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    haystack
        .as_bytes()
        .windows(needle_lower.len())
        .any(|w| w.eq_ignore_ascii_case(needle_lower.as_bytes()))
}

pub struct App {
    pub client: Client,
    pub current_namespace: String,

    pub mode: AppMode,
    pub active_tab: ResourceType,
    pub should_quit: bool,

    pub pod_store: Option<Store<Pod>>,
    pub deployment_store: Option<Store<Deployment>>,
    pub secret_store: Option<Store<Secret>>,
    pub current_context: String,
    pub pending_context: Option<String>,

    pub event_tx: UnboundedSender<KubeResourceEvent>,

    pub items: Vec<KubeResource>,
    pub filtered_items: Vec<KubeResource>,
    pub table_state: TableState,
    pub filter_query: String,
    pub selected_indices: HashSet<usize>,

    pub selected_secret_decoded: Option<Vec<(String, String)>>,
    pub log_buffer: VecDeque<String>,
    pub log_task: Option<AbortHandle>,
    pub log_scroll_offset: Option<usize>,

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

    pub secret_scroll: usize,
    pub secret_table_state: TableState,
    pub secret_revealed: bool,

    pub scale_input: String,

    pub pending_action: Option<PendingAction>,

    pub describe_content: Vec<String>,
    pub describe_scroll: usize,

    pub shell_session: Option<ShellSession>,

    pub clipboard_clear_task: Option<AbortHandle>,

    pub log_pod_name: String,
    pub log_namespace: String,
    pub log_tail_lines: i64,
    pub log_loading_history: bool,
    pub log_generation: u64,
    pub log_history_exhausted: bool,
    pub log_history_task: Option<AbortHandle>,

    pub status_filter: HashSet<String>,
    pub status_filter_items: Vec<(String, usize)>,
    pub status_filter_selected: HashSet<usize>,
    pub status_filter_state: ListState,

    pub log_search_query: String,
    pub log_search_input: String,
    pub log_search_match_line: Option<usize>,
    pub log_search_pending: bool,

    pub app_state: AppState,
}

impl App {
    pub async fn new(
        client: Client,
    ) -> anyhow::Result<(
        Self,
        tokio::sync::mpsc::UnboundedReceiver<KubeResourceEvent>,
    )> {
        let namespace =
            crate::k8s::config::get_context_namespace().unwrap_or_else(|_| "default".to_string());
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        Ok((
            Self {
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
                current_context: "default".into(),
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
                log_pod_name: String::new(),
                log_namespace: String::new(),
                log_tail_lines: 100,
                log_loading_history: false,
                log_generation: 0,
                log_history_exhausted: false,
                log_history_task: None,
                status_filter: HashSet::new(),
                status_filter_items: Vec::new(),
                status_filter_selected: HashSet::new(),
                status_filter_state: ListState::default(),
                log_search_query: String::new(),
                log_search_input: String::new(),
                log_search_match_line: None,
                log_search_pending: false,
                app_state: AppState::load(),
            },
            rx,
        ))
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
        self.status_filter.clear();
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
        self.log_tail_lines = 100;
        self.log_loading_history = false;
        self.log_generation += 1;
        self.log_history_exhausted = false;
        self.log_search_query.clear();
        self.log_search_input.clear();
        self.log_search_match_line = None;
        self.log_search_pending = false;
        self.log_pod_name = pod_name.to_owned();
        self.log_namespace = namespace.to_owned();
        self.mode = AppMode::LogView;

        let abort = crate::k8s::actions::stream_pod_logs(
            self.client.clone(),
            namespace,
            pod_name,
            self.event_tx.clone(),
            self.log_tail_lines,
        );
        self.log_task = Some(abort);
    }

    pub fn load_more_history(&mut self) {
        if self.log_loading_history || self.log_history_exhausted {
            return;
        }
        if self.log_tail_lines >= MAX_LOG_LINES as i64 {
            self.log_history_exhausted = true;
            return;
        }
        self.log_loading_history = true;
        self.log_tail_lines += 100;
        let handle = crate::k8s::actions::fetch_log_history(
            self.client.clone(),
            &self.log_namespace,
            &self.log_pod_name,
            self.log_tail_lines,
            self.log_generation,
            self.event_tx.clone(),
        );
        self.log_history_task = Some(handle);
    }

    pub fn merge_log_history(&mut self, generation: u64, lines: Vec<String>) {
        if generation != self.log_generation {
            self.log_loading_history = false;
            return;
        }

        if lines.len() < self.log_tail_lines as usize {
            self.log_history_exhausted = true;
        }

        let overlap_idx = self
            .log_buffer
            .front()
            .and_then(|first| lines.iter().rposition(|l| l == first))
            .unwrap_or(lines.len());

        let available = MAX_LOG_LINES.saturating_sub(self.log_buffer.len());
        let prepend_count = overlap_idx.min(available);

        if prepend_count == 0 {
            self.log_history_exhausted = true;
            self.log_loading_history = false;
            self.resolve_pending_search(0);
            return;
        }

        let start = overlap_idx - prepend_count;
        for line in lines[start..overlap_idx].iter().rev() {
            self.log_buffer.push_front(line.clone());
        }

        if let Some(offset) = &mut self.log_scroll_offset {
            *offset += prepend_count;
        }
        if let Some(m) = &mut self.log_search_match_line {
            *m += prepend_count;
        }

        self.log_loading_history = false;
        self.resolve_pending_search(prepend_count);
    }

    fn resolve_pending_search(&mut self, new_line_count: usize) {
        if !self.log_search_pending {
            return;
        }
        self.log_search_pending = false;
        if new_line_count == 0 {
            if self.log_history_exhausted {
                self.set_error("Not found".to_string());
            }
            return;
        }
        let needle = &self.log_search_query;
        if needle.is_empty() {
            return;
        }
        for idx in (0..new_line_count).rev() {
            if contains_ascii_ci(&self.log_buffer[idx], needle) {
                self.log_search_match_line = Some(idx);
                let visible = self.log_visible_height();
                self.scroll_to_line(idx, visible);
                return;
            }
        }
        if self.log_history_exhausted {
            self.set_error("Not found".to_string());
        } else {
            self.set_success(format!(
                "Not found in {} loaded lines, press n to load more",
                self.log_buffer.len()
            ));
        }
    }

    pub fn abort_log_stream(&mut self) {
        if let Some(handle) = self.log_task.take() {
            handle.abort();
        }
        if let Some(handle) = self.log_history_task.take() {
            handle.abort();
        }
        self.log_search_pending = false;
    }

    pub fn load_namespaces(&self) {
        let client = self.client.clone();
        let current_ns = self.current_namespace.clone();
        let ctx = self.current_context.clone();
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            use k8s_openapi::api::core::v1::Namespace;
            use kube::Api;
            use kube::api::ListParams;
            let ns_api: Api<Namespace> = Api::all(client);
            if let Ok(ns_list) = ns_api.list(&ListParams::default()).await {
                let namespaces: Vec<String> = ns_list
                    .iter()
                    .map(|n| n.metadata.name.clone().unwrap_or_default())
                    .collect();
                let _ = tx.send(KubeResourceEvent::NamespacesLoaded(namespaces));
                return;
            }

            if let Ok(output) = tokio::process::Command::new("kubectl")
                .args([
                    "get",
                    "namespaces",
                    "--context",
                    &ctx,
                    "-o",
                    "jsonpath={.items[*].metadata.name}",
                ])
                .output()
                .await
                && output.status.success()
            {
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

            let _ = tx.send(KubeResourceEvent::NamespacesLoaded(vec![current_ns]));
        });
    }

    pub fn update_namespace_filter(&mut self) {
        if self.namespace_input.is_empty() {
            self.filtered_namespaces
                .clone_from(&self.available_namespaces);
        } else {
            let query = self.namespace_input.to_lowercase();
            self.filtered_namespaces = self
                .available_namespaces
                .iter()
                .filter(|ns| ns.to_lowercase().contains(&query))
                .cloned()
                .collect();
        }
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
            if self.last_success.is_some() && elapsed >= 5 {
                self.last_success = None;
                if self.last_error.is_none() {
                    self.message_time = None;
                }
            }
            if let Some(err) = &self.last_error
                && !err.starts_with("Access denied")
                && elapsed >= 15
            {
                self.last_error = None;
                self.message_time = None;
            }
        }
    }

    pub fn start_shell(&mut self, pod_name: &str, namespace: &str) {
        use portable_pty::CommandBuilder;
        let mut cmd = CommandBuilder::new("kubectl");
        cmd.args([
            "exec",
            "-it",
            pod_name,
            "-n",
            namespace,
            "--context",
            &self.current_context,
            "--",
            "sh",
        ]);
        self.spawn_pty_session(cmd);
    }

    pub fn start_kubectl_edit(&mut self, kind: &str, name: &str, namespace: &str) {
        use portable_pty::CommandBuilder;
        let mut cmd = CommandBuilder::new("kubectl");
        cmd.args([
            "edit",
            kind,
            name,
            "-n",
            namespace,
            "--context",
            &self.current_context,
        ]);
        self.spawn_pty_session(cmd);
    }

    fn spawn_pty_session(&mut self, cmd: portable_pty::CommandBuilder) {
        use portable_pty::{PtySize, native_pty_system};

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
                        if tx
                            .send(KubeResourceEvent::ShellOutput(buf[..n].to_vec()))
                            .is_err()
                        {
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
            if let Some(offset) = &mut self.log_scroll_offset {
                *offset = offset.saturating_sub(1);
            }
        }
        self.log_buffer.push_back(line);
    }

    pub fn log_search_next(&mut self) {
        let visible = self.log_visible_height();
        self.log_search_next_with_height(visible);
    }

    pub(crate) fn log_search_next_with_height(&mut self, visible: usize) {
        if self.log_search_query.is_empty() || self.log_buffer.is_empty() {
            return;
        }
        self.log_search_pending = false;
        let needle = &self.log_search_query;
        let len = self.log_buffer.len();
        let start = self
            .log_search_match_line
            .and_then(|m| m.checked_sub(1))
            .unwrap_or_else(|| {
                self.log_scroll_offset
                    .map(|o| (o + visible).min(len).saturating_sub(1))
                    .unwrap_or(len.saturating_sub(1))
            });
        for idx in (0..=start).rev() {
            if contains_ascii_ci(&self.log_buffer[idx], needle) {
                self.log_search_match_line = Some(idx);
                self.scroll_to_line(idx, visible);
                return;
            }
        }
        if self.log_history_exhausted {
            self.set_error("No more matches".to_string());
        } else {
            self.log_search_pending = true;
            self.load_more_history();
        }
    }

    pub fn log_search_prev(&mut self) {
        let visible = self.log_visible_height();
        self.log_search_prev_with_height(visible);
    }

    pub(crate) fn log_search_prev_with_height(&mut self, visible: usize) {
        if self.log_search_query.is_empty() || self.log_buffer.is_empty() {
            return;
        }
        self.log_search_pending = false;
        let needle = &self.log_search_query;
        let len = self.log_buffer.len();
        let start = self
            .log_search_match_line
            .map(|m| m + 1)
            .unwrap_or_else(|| {
                self.log_scroll_offset
                    .unwrap_or(len.saturating_sub(visible))
            });
        for idx in start..len {
            if contains_ascii_ci(&self.log_buffer[idx], needle) {
                self.log_search_match_line = Some(idx);
                self.scroll_to_line(idx, visible);
                return;
            }
        }
        self.set_error("No more matches".to_string());
    }

    fn log_visible_height(&self) -> usize {
        crossterm::terminal::size()
            .map(|(_, h)| (h as usize).saturating_sub(LOG_CHROME_LINES))
            .unwrap_or(20)
    }

    fn scroll_to_line(&mut self, idx: usize, visible: usize) {
        let len = self.log_buffer.len();
        let centered = idx.saturating_sub(visible / 2);
        let max = len.saturating_sub(visible);
        self.log_scroll_offset = Some(centered.min(max));
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
            log_pod_name: String::new(),
            log_namespace: String::new(),
            log_tail_lines: 100,
            log_loading_history: false,
            log_generation: 0,
            log_history_exhausted: false,
            log_history_task: None,
            status_filter: HashSet::new(),
            status_filter_items: Vec::new(),
            status_filter_selected: HashSet::new(),
            status_filter_state: ListState::default(),
            log_search_query: String::new(),
            log_search_input: String::new(),
            log_search_match_line: None,
            log_search_pending: false,
            app_state: AppState::default(),
        }
    }

    pub fn pod_phase(p: &Pod) -> &str {
        p.status
            .as_ref()
            .and_then(|s| s.phase.as_deref())
            .unwrap_or("Unknown")
    }

    pub fn build_status_filter_items(&mut self) {
        let mut counts: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for item in &self.items {
            if let KubeResource::Pod(p) = item {
                *counts.entry(Self::pod_phase(p).to_owned()).or_default() += 1;
            }
        }
        self.status_filter_items = counts.into_iter().collect();
        self.status_filter_selected = self
            .status_filter_items
            .iter()
            .enumerate()
            .filter(|(_, (phase, _))| self.status_filter.contains(phase))
            .map(|(i, _)| i)
            .collect();
    }

    pub fn update_filter(&mut self) {
        self.selected_indices.clear();
        let has_status = self.active_tab == ResourceType::Pod && !self.status_filter.is_empty();
        let has_query = !self.filter_query.is_empty();

        if !has_status && !has_query {
            self.filtered_items.clone_from(&self.items);
        } else {
            let query = self.filter_query.to_lowercase();
            self.filtered_items = self
                .items
                .iter()
                .filter(|item| {
                    if has_status
                        && let KubeResource::Pod(p) = item
                        && !self.status_filter.contains(Self::pod_phase(p))
                    {
                        return false;
                    }
                    if has_query {
                        return item.name().to_lowercase().contains(&query);
                    }
                    true
                })
                .cloned()
                .collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::KubeResource;
    use k8s_openapi::ByteString;
    use k8s_openapi::api::core::v1::{Pod, Secret};
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
        app.items = vec![
            make_pod("nginx"),
            make_pod("redis"),
            make_pod("nginx-proxy"),
        ];
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
        assert_eq!(app.log_buffer[0], "line100");
    }

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
        app.table_state.select(Some(5));

        assert!(app.get_selected_resource().is_none());
    }

    #[tokio::test]
    async fn decode_selected_secret_extracts_data() {
        let mut app = App::new_test();
        app.active_tab = ResourceType::Secret;
        app.filtered_items = vec![make_secret(
            "my-secret",
            vec![("user", "admin"), ("pass", "s3cret")],
        )];
        app.table_state.select(Some(0));

        app.decode_selected_secret();

        let decoded = app.selected_secret_decoded.unwrap();
        assert_eq!(decoded.len(), 2);
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

    #[tokio::test]
    async fn abort_log_stream_clears_handle() {
        let mut app = App::new_test();
        app.abort_log_stream();
        assert!(app.log_task.is_none());
    }

    #[tokio::test]
    async fn new_app_starts_dirty() {
        let app = App::new_test();
        assert!(app.dirty);
    }

    #[tokio::test]
    async fn merge_log_history_prepends_new_lines() {
        let mut app = App::new_test();
        app.log_generation = 1;
        app.log_tail_lines = 200;
        for line in ["line3", "line4", "line5"] {
            app.log_buffer.push_back(line.to_string());
        }
        app.log_scroll_offset = Some(0);
        app.log_loading_history = true;

        let history = vec![
            "line1".into(),
            "line2".into(),
            "line3".into(),
            "line4".into(),
            "line5".into(),
        ];
        app.merge_log_history(1, history);

        assert_eq!(app.log_buffer.len(), 5);
        assert_eq!(app.log_buffer[0], "line1");
        assert_eq!(app.log_buffer[1], "line2");
        assert_eq!(app.log_buffer[2], "line3");
        assert_eq!(app.log_scroll_offset, Some(2));
        assert!(!app.log_loading_history);
    }

    #[tokio::test]
    async fn merge_log_history_discards_wrong_generation() {
        let mut app = App::new_test();
        app.log_generation = 2;
        app.log_buffer.push_back("current".into());
        app.log_loading_history = true;

        app.merge_log_history(1, vec!["old".into(), "current".into()]);

        assert_eq!(app.log_buffer.len(), 1);
        assert_eq!(app.log_buffer[0], "current");
        assert!(!app.log_loading_history);
    }

    #[tokio::test]
    async fn merge_log_history_detects_exhaustion() {
        let mut app = App::new_test();
        app.log_generation = 1;
        app.log_tail_lines = 200;
        app.log_buffer.push_back("line1".into());
        app.log_loading_history = true;

        // Response has fewer lines than requested = pod has no more history
        app.merge_log_history(1, vec!["line1".into()]);

        assert!(app.log_history_exhausted);
    }

    #[tokio::test]
    async fn merge_log_history_caps_at_max_log_lines() {
        let mut app = App::new_test();
        app.log_generation = 1;
        app.log_tail_lines = 200;
        // Fill buffer near capacity
        for i in 0..MAX_LOG_LINES - 2 {
            app.log_buffer.push_back(format!("existing{i}"));
        }
        app.log_loading_history = true;

        // History offers 10 new lines, but only 2 can fit
        let mut history: Vec<String> = (0..10).map(|i| format!("new{i}")).collect();
        history.push("existing0".into());
        app.merge_log_history(1, history);

        assert_eq!(app.log_buffer.len(), MAX_LOG_LINES);
        assert_eq!(app.log_buffer[0], "new8");
        assert_eq!(app.log_buffer[1], "new9");
        assert_eq!(app.log_buffer[2], "existing0");
    }

    #[tokio::test]
    async fn push_log_line_adjusts_scroll_on_eviction() {
        let mut app = App::new_test();
        for i in 0..MAX_LOG_LINES {
            app.log_buffer.push_back(format!("line{i}"));
        }
        app.log_scroll_offset = Some(50);

        app.push_log_line("new".into());

        assert_eq!(app.log_buffer.len(), MAX_LOG_LINES);
        assert_eq!(app.log_scroll_offset, Some(49));
    }

    #[tokio::test]
    async fn load_more_history_skips_when_exhausted() {
        let mut app = App::new_test();
        app.log_history_exhausted = true;
        app.log_tail_lines = 100;

        app.load_more_history();

        assert_eq!(app.log_tail_lines, 100);
        assert!(!app.log_loading_history);
    }

    #[tokio::test]
    async fn load_more_history_caps_at_max() {
        let mut app = App::new_test();
        app.log_tail_lines = MAX_LOG_LINES as i64;

        app.load_more_history();

        assert!(app.log_history_exhausted);
        assert!(!app.log_loading_history);
        assert_eq!(app.log_tail_lines, MAX_LOG_LINES as i64);
    }

    #[tokio::test]
    async fn log_search_next_finds_match() {
        let mut app = App::new_test();
        for i in 0..50 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_buffer.push_back("target match here".to_string());
        for i in 51..200 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_scroll_offset = Some(100);
        app.log_search_query = "target".to_string();

        app.log_search_next_with_height(20);

        assert_eq!(app.log_search_match_line, Some(50));
        assert_eq!(app.log_scroll_offset, Some(40));
    }

    #[tokio::test]
    async fn log_search_prev_finds_match() {
        let mut app = App::new_test();
        for i in 0..80 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_buffer.push_back("target match here".to_string());
        for i in 81..200 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_scroll_offset = Some(50);
        app.log_search_query = "target".to_string();

        app.log_search_prev_with_height(20);

        assert_eq!(app.log_search_match_line, Some(80));
        assert_eq!(app.log_scroll_offset, Some(70));
    }

    #[tokio::test]
    async fn log_search_next_finds_above_scroll() {
        let mut app = App::new_test();
        app.log_buffer.push_back("target line".to_string());
        for i in 1..50 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_scroll_offset = Some(10);
        app.log_search_query = "target".to_string();

        app.log_search_next_with_height(20);

        assert_eq!(app.log_scroll_offset, Some(0));
    }

    #[tokio::test]
    async fn log_search_case_insensitive() {
        let mut app = App::new_test();
        for i in 0..50 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_buffer.push_back("TARGET MATCH".to_string());
        for i in 51..200 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_scroll_offset = Some(100);
        app.log_search_query = "target".to_string();

        app.log_search_next_with_height(20);

        assert_eq!(app.log_search_match_line, Some(50));
        assert_eq!(app.log_scroll_offset, Some(40));
    }

    #[tokio::test]
    async fn log_search_next_empty_buffer_noop() {
        let mut app = App::new_test();
        app.log_search_query = "test".to_string();
        app.log_scroll_offset = Some(0);

        app.log_search_next_with_height(20);

        assert_eq!(app.log_scroll_offset, Some(0));
    }

    #[tokio::test]
    async fn log_search_next_empty_query_noop() {
        let mut app = App::new_test();
        app.log_buffer.push_back("some line".to_string());
        app.log_scroll_offset = Some(0);
        app.log_search_query.clear();

        app.log_search_next_with_height(20);

        assert_eq!(app.log_scroll_offset, Some(0));
    }

    #[tokio::test]
    async fn log_search_prev_empty_buffer_noop() {
        let mut app = App::new_test();
        app.log_search_query = "test".to_string();
        app.log_scroll_offset = Some(0);

        app.log_search_prev_with_height(20);

        assert_eq!(app.log_scroll_offset, Some(0));
    }

    #[tokio::test]
    async fn log_search_prev_empty_query_noop() {
        let mut app = App::new_test();
        app.log_buffer.push_back("some line".to_string());
        app.log_scroll_offset = Some(0);
        app.log_search_query.clear();

        app.log_search_prev_with_height(20);

        assert_eq!(app.log_scroll_offset, Some(0));
    }

    #[tokio::test]
    async fn log_search_next_advances_past_current() {
        let mut app = App::new_test();
        for i in 0..200 {
            if i == 50 || i == 100 {
                app.log_buffer.push_back(format!("error at {i}"));
            } else {
                app.log_buffer.push_back(format!("line {i}"));
            }
        }
        app.log_scroll_offset = Some(150);
        app.log_search_query = "error".to_string();

        app.log_search_next_with_height(20);
        assert_eq!(app.log_search_match_line, Some(100));
        assert_eq!(app.log_scroll_offset, Some(90));

        app.log_search_next_with_height(20);
        assert_eq!(app.log_search_match_line, Some(50));
        assert_eq!(app.log_scroll_offset, Some(40));

        app.log_search_next_with_height(20);
        assert!(app.log_search_pending);
    }

    #[tokio::test]
    async fn log_search_prev_advances_past_current() {
        let mut app = App::new_test();
        for i in 0..200 {
            if i == 50 || i == 100 {
                app.log_buffer.push_back(format!("error at {i}"));
            } else {
                app.log_buffer.push_back(format!("line {i}"));
            }
        }
        app.log_search_match_line = Some(50);
        app.log_scroll_offset = Some(40);
        app.log_search_query = "error".to_string();

        app.log_search_prev_with_height(20);
        assert_eq!(app.log_search_match_line, Some(100));
        assert_eq!(app.log_scroll_offset, Some(90));

        app.log_search_prev_with_height(20);
        assert!(app.last_error.as_ref().unwrap().contains("No more matches"));
    }

    #[tokio::test]
    async fn log_search_not_found_sets_pending() {
        let mut app = App::new_test();
        for i in 0..50 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_search_query = "nothere".to_string();

        app.log_search_next_with_height(20);

        assert!(app.log_search_pending);
    }

    #[tokio::test]
    async fn log_search_exhausted_shows_not_found() {
        let mut app = App::new_test();
        for i in 0..50 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_history_exhausted = true;
        app.log_search_query = "nothere".to_string();

        app.log_search_next_with_height(20);

        assert!(!app.log_search_pending);
        assert!(app.last_error.as_ref().unwrap().contains("No more matches"));
    }

    #[tokio::test]
    async fn log_search_pending_resolved_on_merge() {
        let mut app = App::new_test();
        app.log_buffer.push_back("existing".to_string());
        app.log_generation = 1;
        app.log_tail_lines = 200;
        app.log_search_query = "target".to_string();
        app.log_search_pending = true;
        app.log_loading_history = true;

        let history = vec!["target found".into(), "existing".into()];
        app.merge_log_history(1, history);

        assert!(!app.log_search_pending);
        assert_eq!(app.log_search_match_line, Some(0));
    }

    #[tokio::test]
    async fn log_search_pending_no_match_in_new_lines() {
        let mut app = App::new_test();
        app.log_buffer.push_back("existing".to_string());
        app.log_generation = 1;
        app.log_tail_lines = 2;
        app.log_search_query = "nothere".to_string();
        app.log_search_pending = true;
        app.log_loading_history = true;

        let history = vec!["other line".into(), "existing".into()];
        app.merge_log_history(1, history);

        assert!(!app.log_search_pending);
        assert!(app.last_success.as_ref().unwrap().contains("press n"));
    }

    #[tokio::test]
    async fn log_search_pending_exhausted_no_match() {
        let mut app = App::new_test();
        app.log_buffer.push_back("existing".to_string());
        app.log_generation = 1;
        app.log_tail_lines = 200;
        app.log_search_query = "nothere".to_string();
        app.log_search_pending = true;
        app.log_loading_history = true;

        let history = vec!["other line".into(), "existing".into()];
        app.merge_log_history(1, history);

        assert!(!app.log_search_pending);
        assert!(app.last_error.as_ref().unwrap().contains("Not found"));
    }

    #[tokio::test]
    async fn log_search_match_line_adjusted_on_history_prepend() {
        let mut app = App::new_test();
        app.log_buffer.push_back("match line".to_string());
        app.log_buffer.push_back("other".to_string());
        app.log_generation = 1;
        app.log_tail_lines = 200;
        app.log_search_match_line = Some(0);
        app.log_loading_history = true;

        let history = vec!["new1".into(), "new2".into(), "match line".into()];
        app.merge_log_history(1, history);

        assert_eq!(app.log_search_match_line, Some(2));
    }

    #[tokio::test]
    async fn log_search_next_single_match_loads_history() {
        let mut app = App::new_test();
        for i in 0..50 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_buffer[20] = "error here".to_string();
        app.log_search_query = "error".to_string();

        app.log_search_next_with_height(20);
        assert_eq!(app.log_search_match_line, Some(20));

        app.log_search_next_with_height(20);
        assert!(app.log_search_pending);
    }

    #[tokio::test]
    async fn log_search_next_single_match_stops_when_exhausted() {
        let mut app = App::new_test();
        for i in 0..50 {
            app.log_buffer.push_back(format!("line {i}"));
        }
        app.log_buffer[20] = "error here".to_string();
        app.log_history_exhausted = true;
        app.log_search_query = "error".to_string();

        app.log_search_next_with_height(20);
        assert_eq!(app.log_search_match_line, Some(20));

        app.log_search_next_with_height(20);
        assert_eq!(app.log_search_match_line, Some(20));
        assert!(!app.log_search_pending);
    }
}
