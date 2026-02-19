use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AppState {
    #[serde(default)]
    pub namespaces: HashMap<String, Vec<String>>,
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

    pub fn add_namespace(&mut self, context: &str, namespace: &str) {
        let entry = self.namespaces.entry(context.to_string()).or_default();
        if !entry.contains(&namespace.to_string()) {
            entry.push(namespace.to_string());
            entry.sort();
        }
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
    fn get_namespaces_empty_context() {
        let state = AppState::default();
        assert!(state.get_namespaces("unknown").is_empty());
    }
}
