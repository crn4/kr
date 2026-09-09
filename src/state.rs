use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

static SAVE_SEQ: AtomicU64 = AtomicU64::new(0);
static LAST_WRITTEN: Mutex<u64> = Mutex::new(0);

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AppState {
    #[serde(default)]
    pub namespaces: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub last_namespace: HashMap<String, String>,
    #[serde(skip)]
    pub no_persist: bool,
}

fn write_atomically(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }

    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let write = (|| {
        let mut file = options.open(&tmp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()
    })();

    if let Err(e) = write {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }

    std::fs::rename(&tmp, path)
}

fn write_if_newer(last_written: &Mutex<u64>, path: &Path, contents: &str, seq: u64) -> bool {
    let mut last = last_written
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if seq <= *last {
        tracing::debug!("skipping state snapshot {seq}, {last} is already on disk");
        return false;
    }
    match write_atomically(path, contents) {
        Ok(()) => {
            *last = seq;
            true
        }
        Err(e) => {
            tracing::warn!("failed to persist state to {}: {e}", path.display());
            false
        }
    }
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
        let reason = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(state) => return state,
                Err(e) => e.to_string(),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(e) => e.to_string(),
        };

        let salvaged = path.with_extension("json.corrupt");
        tracing::warn!(
            "{} is not usable state ({reason}); keeping it as {} and starting fresh",
            path.display(),
            salvaged.display()
        );
        let _ = std::fs::rename(&path, &salvaged);
        Self::default()
    }

    pub fn save(&self) {
        if self.no_persist {
            return;
        }
        let path = state_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let seq = SAVE_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
            tokio::task::spawn_blocking(move || {
                write_if_newer(&LAST_WRITTEN, &path, &json, seq);
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
    fn atomic_write_creates_the_file_private() {
        let dir = std::env::temp_dir().join(format!("kr-state-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("state.json");

        write_atomically(&path, r#"{"namespaces":{}}"#).expect("write failed");

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{"namespaces":{}}"#
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "state file must not be world-readable");
            let dir_mode = std::fs::metadata(&dir).unwrap().permissions().mode();
            assert_eq!(dir_mode & 0o777, 0o700);
        }
        assert!(
            std::fs::read_dir(&dir)
                .unwrap()
                .filter_map(Result::ok)
                .all(|e| e.file_name() == "state.json"),
            "the temp file must not survive a successful write"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_replaces_existing_content() {
        let dir = std::env::temp_dir().join(format!("kr-state-replace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("state.json");

        write_atomically(&path, "first-and-longer").unwrap();
        write_atomically(&path, "second").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_older_snapshot_never_overwrites_a_newer_one() {
        let dir = std::env::temp_dir().join(format!("kr-state-seq-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("state.json");
        let last_written = Mutex::new(0);

        assert!(write_if_newer(&last_written, &path, "second", 2));
        assert!(!write_if_newer(&last_written, &path, "first", 1));

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");

        assert!(write_if_newer(&last_written, &path, "third", 3));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "third");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_poisoned_save_lock_does_not_stop_later_writes() {
        let dir = std::env::temp_dir().join(format!("kr-state-poison-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("state.json");
        let last_written = std::sync::Arc::new(Mutex::new(0u64));

        let poisoner = std::sync::Arc::clone(&last_written);
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock().unwrap();
            panic!("poison the lock");
        })
        .join();
        assert!(last_written.is_poisoned());

        assert!(write_if_newer(&last_written, &path, "after", 1));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "after");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_namespaces_empty_context() {
        let state = AppState::default();
        assert!(state.get_namespaces("unknown").is_empty());
    }
}
