use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AppState {
    #[serde(default)]
    pub namespaces: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub last_namespace: HashMap<String, String>,
    #[serde(skip)]
    pub no_persist: bool,
}

fn state_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("kr");
    path.push("state.json");
    path
}

impl AppState {
    pub fn load() -> Self {
        let path = state_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if self.no_persist {
            return;
        }
        let path = state_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            tokio::task::spawn_blocking(move || {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = std::fs::set_permissions(
                            parent,
                            std::fs::Permissions::from_mode(0o700),
                        );
                    }
                }
                let tmp = path.with_extension("tmp");
                if std::fs::write(&tmp, &json).is_ok() {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ =
                            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
                    }
                    let _ = std::fs::rename(&tmp, &path);
                }
            });
        }
    }

    pub fn get_namespaces(&self, context: &str) -> Vec<String> {
        self.namespaces.get(context).cloned().unwrap_or_default()
    }

    pub fn last_namespace(&self, context: &str) -> Option<&str> {
        self.last_namespace.get(context).map(String::as_str)
    }

    pub fn set_last_namespace(&mut self, context: &str, namespace: &str) {
        self.last_namespace
            .insert(context.to_string(), namespace.to_string());
    }

    pub fn add_namespace(&mut self, context: &str, namespace: &str) {
        let entry = self.namespaces.entry(context.to_string()).or_default();
        if !entry.contains(&namespace.to_string()) {
            entry.push(namespace.to_string());
            entry.sort();
        }
    }

    pub fn replace_namespaces(
        &mut self,
        context: &str,
        discovered: &[String],
        superseded: &[String],
        keep: &str,
    ) -> Vec<String> {
        let entry = self.namespaces.entry(context.to_string()).or_default();
        entry.retain(|ns| !superseded.contains(ns));
        entry.extend(discovered.iter().cloned());
        if !keep.is_empty() {
            entry.push(keep.to_string());
        }
        entry.sort();
        entry.dedup();
        entry.clone()
    }

    pub fn merge_namespaces(&mut self, context: &str, discovered: &[String]) -> Vec<String> {
        let entry = self.namespaces.entry(context.to_string()).or_default();
        for ns in discovered {
            if !entry.contains(ns) {
                entry.push(ns.clone());
            }
        }
        entry.sort();
        entry.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_namespace_deduplicates() {
        let mut state = AppState::default();
        state.add_namespace("ctx1", "ns-a");
        state.add_namespace("ctx1", "ns-b");
        state.add_namespace("ctx1", "ns-a");
        assert_eq!(state.get_namespaces("ctx1"), vec!["ns-a", "ns-b"]);
    }

    #[test]
    fn merge_namespaces_combines() {
        let mut state = AppState::default();
        state.add_namespace("ctx1", "saved-ns");
        let merged = state.merge_namespaces("ctx1", &["api-ns".into(), "saved-ns".into()]);
        assert_eq!(merged, vec!["api-ns", "saved-ns"]);
    }

    #[test]
    fn replace_namespaces_drops_stale_entries() {
        let mut state = AppState::default();
        state.add_namespace("ctx1", "gone-ns");
        state.add_namespace("ctx1", "still-ns");
        let result =
            state.replace_namespaces("ctx1", &["still-ns".into()], &["gone-ns".into()], "");
        assert_eq!(result, vec!["still-ns"]);
        assert_eq!(state.get_namespaces("ctx1"), vec!["still-ns"]);
    }

    #[test]
    fn replace_namespaces_keeps_current() {
        let mut state = AppState::default();
        state.add_namespace("ctx1", "gone-ns");
        let result =
            state.replace_namespaces("ctx1", &["new-ns".into()], &["gone-ns".into()], "manual-ns");
        assert_eq!(result, vec!["manual-ns", "new-ns"]);
    }

    #[test]
    fn replace_namespaces_does_not_duplicate_current() {
        let mut state = AppState::default();
        let result = state.replace_namespaces("ctx1", &["ns-a".into()], &[], "ns-a");
        assert_eq!(result, vec!["ns-a"]);
    }

    #[test]
    fn replace_namespaces_isolates_contexts() {
        let mut state = AppState::default();
        state.add_namespace("ctx2", "other-ns");
        state.replace_namespaces("ctx1", &["ns-a".into()], &[], "");
        assert_eq!(state.get_namespaces("ctx2"), vec!["other-ns"]);
    }

    #[test]
    fn last_namespace_roundtrips_per_context() {
        let mut state = AppState::default();
        assert!(state.last_namespace("ctx1").is_none());
        state.set_last_namespace("ctx1", "ns-a");
        state.set_last_namespace("ctx2", "ns-b");
        assert_eq!(state.last_namespace("ctx1"), Some("ns-a"));
        assert_eq!(state.last_namespace("ctx2"), Some("ns-b"));
    }

    #[test]
    fn last_namespace_overwrites() {
        let mut state = AppState::default();
        state.set_last_namespace("ctx1", "old");
        state.set_last_namespace("ctx1", "new");
        assert_eq!(state.last_namespace("ctx1"), Some("new"));
    }

    #[test]
    fn state_without_last_namespace_field_still_loads() {
        let state: AppState = serde_json::from_str(r#"{"namespaces":{"ctx1":["ns-a"]}}"#).unwrap();
        assert_eq!(state.get_namespaces("ctx1"), vec!["ns-a"]);
        assert!(state.last_namespace("ctx1").is_none());
    }

    #[test]
    fn get_namespaces_empty_context() {
        let state = AppState::default();
        assert!(state.get_namespaces("unknown").is_empty());
    }
}
