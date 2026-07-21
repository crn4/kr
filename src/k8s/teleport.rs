use anyhow::{Result, anyhow, bail};
use kube::config::Kubeconfig;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::OnceCell;

const TSH: &str = "tsh";
const TSH_TIMEOUT: Duration = Duration::from_secs(10);
const STATUS_TTL: Duration = Duration::from_secs(5);
const NS_GROUP_PREFIX: &str = "teleport:ns:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub cluster: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum State {
    #[default]
    Unavailable,
    NeedsLogin,
    Available(Vec<String>),
}

impl State {
    pub fn clusters(&self) -> &[String] {
        match self {
            State::Available(clusters) => clusters,
            _ => &[],
        }
    }

    pub fn needs_login(&self) -> bool {
        matches!(self, State::NeedsLogin)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Login {
    Profile,
    KubeCluster(String),
}

async fn capture(args: &[&str], routine_failure: bool) -> Option<String> {
    let output = tokio::time::timeout(
        TSH_TIMEOUT,
        tokio::process::Command::new(TSH)
            .args(args)
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .inspect_err(|_| tracing::warn!("tsh {} timed out after {TSH_TIMEOUT:?}", args.join(" ")))
    .ok()?
    .inspect_err(|e| tracing::warn!("tsh {} failed to spawn: {e}", args.join(" ")))
    .ok()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if routine_failure {
            tracing::debug!("tsh {} exited with {}", args.join(" "), output.status);
        } else {
            tracing::warn!(
                "tsh {} exited with {}: {}",
                args.join(" "),
                output.status,
                stderr.trim()
            );
        }
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub async fn is_installed() -> bool {
    static INSTALLED: OnceCell<bool> = OnceCell::const_new();
    if let Some(installed) = INSTALLED.get() {
        return *installed;
    }
    match tokio::time::timeout(
        TSH_TIMEOUT,
        tokio::process::Command::new(TSH)
            .args(["version", "--client"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status(),
    )
    .await
    {
        Ok(Ok(status)) => {
            let _ = INSTALLED.set(status.success());
            status.success()
        }
        Ok(Err(e)) => {
            tracing::debug!("tsh not available: {e}");
            let _ = INSTALLED.set(false);
            false
        }
        Err(_) => {
            tracing::warn!("tsh version timed out after {TSH_TIMEOUT:?}, not caching");
            false
        }
    }
}

pub fn parse_profile(json: &str) -> Option<Profile> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let cluster = value
        .get("active")?
        .get("cluster")?
        .as_str()
        .filter(|c| !c.is_empty())?
        .to_string();
    Some(Profile { cluster })
}

static STATUS_CACHE: tokio::sync::Mutex<Option<(u64, std::time::Instant, String)>> =
    tokio::sync::Mutex::const_new(None);
static STATUS_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn invalidate_status_cache() {
    STATUS_GENERATION.fetch_add(1, Ordering::Relaxed);
}

async fn status_json() -> Option<String> {
    let generation = STATUS_GENERATION.load(Ordering::Relaxed);
    let mut cache = STATUS_CACHE.lock().await;
    if let Some((cached_generation, fetched_at, json)) = cache.as_ref()
        && *cached_generation == generation
        && fetched_at.elapsed() < STATUS_TTL
    {
        return Some(json.clone());
    }
    let json = capture(&["status", "--format=json"], true).await?;
    *cache = Some((generation, std::time::Instant::now(), json.clone()));
    Some(json)
}

pub async fn active_profile() -> Option<Profile> {
    parse_profile(&status_json().await?)
}

pub fn parse_namespaces(json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(groups) = value
        .get("active")
        .and_then(|a| a.get("kubernetes_groups"))
        .and_then(|g| g.as_array())
    else {
        return Vec::new();
    };
    let mut namespaces: Vec<String> = groups
        .iter()
        .filter_map(|g| g.as_str())
        .filter_map(|g| g.strip_prefix(NS_GROUP_PREFIX))
        .map(|rest| rest.split(':').next().unwrap_or(rest))
        .filter(|ns| !ns.is_empty())
        .map(str::to_string)
        .collect();
    namespaces.sort();
    namespaces.dedup();
    namespaces
}

pub fn is_teleport_context(context: &str, profile_cluster: &str) -> bool {
    context.len() > profile_cluster.len() + 1
        && context.starts_with(profile_cluster)
        && context.as_bytes()[profile_cluster.len()] == b'-'
}

pub async fn grantable_namespaces(context: &str) -> Vec<String> {
    if !is_installed().await {
        return Vec::new();
    }
    let Some(json) = status_json().await else {
        return Vec::new();
    };
    let Some(profile) = parse_profile(&json) else {
        return Vec::new();
    };
    if !is_teleport_context(context, &profile.cluster) {
        return Vec::new();
    }
    parse_namespaces(&json)
}

pub fn parse_kube_clusters(json: &str) -> Vec<String> {
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(json)
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.get("kube_cluster_name").and_then(|v| v.as_str()))
        .filter(|name| !name.is_empty() && !name.starts_with('-'))
        .map(str::to_string)
        .collect()
}

pub async fn list_kube_clusters() -> Option<Vec<String>> {
    capture(&["kube", "ls", "-f", "json"], false)
        .await
        .map(|json| parse_kube_clusters(&json))
}

pub fn context_name(profile_cluster: &str, kube_cluster: &str) -> String {
    format!("{profile_cluster}-{kube_cluster}")
}

pub fn missing_clusters(all: &[String], contexts: &[String], profile_cluster: &str) -> Vec<String> {
    let prefix = format!("{profile_cluster}-");
    let mut missing: Vec<String> = all
        .iter()
        .filter(|cluster| {
            !contexts
                .iter()
                .any(|ctx| ctx.strip_prefix(prefix.as_str()) == Some(cluster.as_str()))
        })
        .cloned()
        .collect();
    missing.sort();
    missing
}

pub fn compose_state(
    profile: Option<&Profile>,
    all: Option<&[String]>,
    contexts: &[String],
    profile_on_disk: bool,
) -> State {
    match profile {
        None if profile_on_disk => State::NeedsLogin,
        None => State::Unavailable,
        Some(profile) => State::Available(missing_clusters(
            all.unwrap_or_default(),
            contexts,
            &profile.cluster,
        )),
    }
}

pub async fn probe() -> State {
    if !is_installed().await {
        return State::Unavailable;
    }
    let profile = active_profile().await;
    let all = match profile {
        Some(_) => list_kube_clusters().await,
        None => None,
    };
    let contexts = read_context_names().await;
    compose_state(
        profile.as_ref(),
        all.as_deref(),
        &contexts,
        has_profile_on_disk(),
    )
}

async fn read_context_names() -> Vec<String> {
    tokio::task::spawn_blocking(|| {
        Kubeconfig::read()
            .map(|c| c.contexts.into_iter().map(|c| c.name).collect())
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default()
}

fn has_profile_on_disk() -> bool {
    dirs::home_dir()
        .map(|h| h.join(".tsh").join("current-profile").exists())
        .unwrap_or(false)
}

pub fn kube_login_args(cluster: &str) -> [&str; 4] {
    ["kube", "login", "--disable-access-request", cluster]
}

fn run_interactive(args: &[&str]) -> Result<()> {
    let spawned = Command::new(TSH).args(args).status();
    invalidate_status_cache();
    let status = spawned?;
    if !status.success() {
        bail!("tsh {} exited with {status}", args.join(" "));
    }
    Ok(())
}

pub fn log_in(login: &Login) -> Result<Option<String>> {
    let cluster = match login {
        Login::Profile => {
            run_interactive(&["login"])?;
            return Ok(None);
        }
        Login::KubeCluster(cluster) => cluster.as_str(),
    };

    if cluster.starts_with('-') {
        bail!("refusing suspicious cluster name '{cluster}'");
    }

    let profile = match blocking_profile() {
        Some(profile) => profile,
        None => {
            eprintln!("Teleport session expired, running 'tsh login'...");
            run_interactive(&["login"])?;
            blocking_profile().ok_or_else(|| anyhow!("no active Teleport profile after login"))?
        }
    };

    run_interactive(&kube_login_args(cluster))?;
    let config = Kubeconfig::read()?;
    resolve_context(&config, &profile.cluster, cluster).map(Some)
}

fn blocking_profile() -> Option<Profile> {
    let output = Command::new(TSH)
        .args(["status", "--format=json"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_profile(&String::from_utf8_lossy(&output.stdout))
}

pub fn resolve_context(
    config: &Kubeconfig,
    profile_cluster: &str,
    kube_cluster: &str,
) -> Result<String> {
    let expected = context_name(profile_cluster, kube_cluster);
    if config.contexts.iter().any(|c| c.name == expected) {
        return Ok(expected);
    }
    config
        .current_context
        .clone()
        .ok_or_else(|| anyhow!("context for '{kube_cluster}' not found in kubeconfig"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(cluster: &str) -> Profile {
        Profile {
            cluster: cluster.to_string(),
        }
    }

    #[test]
    fn parses_active_profile() {
        let json = r#"{"active":{"profile_url":"https://teleport.example.com:443","cluster":"teleport-example"}}"#;
        assert_eq!(parse_profile(json).unwrap().cluster, "teleport-example");
    }

    #[test]
    fn no_profile_when_inactive() {
        assert!(parse_profile(r#"{"active":null}"#).is_none());
        assert!(parse_profile(r#"{"active":{"cluster":""}}"#).is_none());
        assert!(parse_profile(r#"{}"#).is_none());
        assert!(parse_profile("not json").is_none());
    }

    #[test]
    fn parses_namespaces_from_groups() {
        let json = r#"{"active":{"kubernetes_groups":[
            "teleport:ns:team-a:readwrite",
            "teleport:ns:team-a:secret:readonly",
            "teleport:ns:team-b:readwrite"
        ]}}"#;
        assert_eq!(parse_namespaces(json), vec!["team-a", "team-b"]);
    }

    #[test]
    fn ignores_non_namespace_groups() {
        let json = r#"{"active":{"kubernetes_groups":["system:masters","teleport:ns:a:readwrite","teleport:ns::x"]}}"#;
        assert_eq!(parse_namespaces(json), vec!["a"]);
    }

    #[test]
    fn no_namespaces_without_groups() {
        assert!(parse_namespaces(r#"{"active":{}}"#).is_empty());
        assert!(parse_namespaces(r#"{}"#).is_empty());
        assert!(parse_namespaces("not json").is_empty());
    }

    #[test]
    fn parses_kube_clusters() {
        let json = r#"[{"kube_cluster_name":"k8s.a","selected":false},{"kube_cluster_name":"k8s.b","selected":true}]"#;
        assert_eq!(parse_kube_clusters(json), vec!["k8s.a", "k8s.b"]);
    }

    #[test]
    fn rejects_flag_like_cluster_names() {
        let json = r#"[{"kube_cluster_name":"--insecure"},{"kube_cluster_name":""},{"kube_cluster_name":"k8s.ok"}]"#;
        assert_eq!(parse_kube_clusters(json), vec!["k8s.ok"]);
    }

    #[test]
    fn empty_kube_clusters_on_bad_json() {
        assert!(parse_kube_clusters("{}").is_empty());
        assert!(parse_kube_clusters("").is_empty());
    }

    #[test]
    fn kube_login_always_disables_access_requests() {
        let args = kube_login_args("k8s.alpha");
        assert!(
            args.contains(&"--disable-access-request"),
            "tsh kube login files a server-side access request by default; \
             selecting a row in a TUI list must never do that"
        );
        assert_eq!(args.last(), Some(&"k8s.alpha"));
    }

    #[test]
    fn builds_context_name() {
        assert_eq!(
            context_name("teleport-example", "k8s.alpha"),
            "teleport-example-k8s.alpha"
        );
    }

    #[test]
    fn recognizes_teleport_contexts() {
        assert!(is_teleport_context(
            "teleport-example-k8s.alpha",
            "teleport-example"
        ));
        assert!(!is_teleport_context("docker-desktop", "teleport-example"));
        assert!(!is_teleport_context("teleport-example", "teleport-example"));
        assert!(!is_teleport_context(
            "teleport-example-",
            "teleport-example"
        ));
        assert!(!is_teleport_context(
            "teleport-examplex-a",
            "teleport-example"
        ));
    }

    #[test]
    fn computes_missing_clusters() {
        let all = vec![
            "k8s.beta".to_string(),
            "k8s.alpha".to_string(),
            "k8s.gamma".to_string(),
        ];
        let contexts = vec!["teleport-example-k8s.alpha".to_string()];
        assert_eq!(
            missing_clusters(&all, &contexts, "teleport-example"),
            vec!["k8s.beta", "k8s.gamma"]
        );
    }

    #[test]
    fn no_missing_when_all_logged_in() {
        let all = vec!["k8s.a".to_string()];
        let contexts = vec!["tp-k8s.a".to_string()];
        assert!(missing_clusters(&all, &contexts, "tp").is_empty());
    }

    #[test]
    fn compose_state_needs_login_only_with_profile_on_disk() {
        assert_eq!(compose_state(None, None, &[], true), State::NeedsLogin);
        assert_eq!(compose_state(None, None, &[], false), State::Unavailable);
    }

    #[test]
    fn compose_state_available_lists_missing() {
        let all = vec!["k8s.a".to_string(), "k8s.b".to_string()];
        let contexts = vec!["tp-k8s.a".to_string()];
        let state = compose_state(Some(&profile("tp")), Some(&all), &contexts, true);
        assert_eq!(state, State::Available(vec!["k8s.b".to_string()]));
        assert_eq!(state.clusters(), ["k8s.b"]);
        assert!(!state.needs_login());
    }

    #[test]
    fn compose_state_available_with_failed_listing() {
        let state = compose_state(Some(&profile("tp")), None, &[], true);
        assert_eq!(state, State::Available(Vec::new()));
    }

    #[test]
    fn state_accessors_on_unavailable() {
        assert!(State::Unavailable.clusters().is_empty());
        assert!(!State::Unavailable.needs_login());
        assert!(State::NeedsLogin.needs_login());
        assert!(State::NeedsLogin.clusters().is_empty());
    }

    fn kubeconfig_with(contexts: &[&str], current: Option<&str>) -> Kubeconfig {
        Kubeconfig {
            contexts: contexts
                .iter()
                .map(|name| kube::config::NamedContext {
                    name: name.to_string(),
                    context: None,
                })
                .collect(),
            current_context: current.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn resolve_context_prefers_expected_name() {
        let config = kubeconfig_with(&["tp-k8s.a", "other"], Some("other"));
        assert_eq!(resolve_context(&config, "tp", "k8s.a").unwrap(), "tp-k8s.a");
    }

    #[test]
    fn resolve_context_falls_back_to_current() {
        let config = kubeconfig_with(&["custom-name"], Some("custom-name"));
        assert_eq!(
            resolve_context(&config, "tp", "k8s.a").unwrap(),
            "custom-name"
        );
    }

    #[test]
    fn resolve_context_errors_without_current() {
        let config = kubeconfig_with(&[], None);
        assert!(resolve_context(&config, "tp", "k8s.a").is_err());
    }
}
