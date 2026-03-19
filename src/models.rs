use k8s_openapi::api::{
    apps::v1::Deployment,
    core::v1::{Pod, Secret},
};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    pub fn toggle(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }

    pub fn indicator(self) -> &'static str {
        match self {
            Self::Asc => " ▲",
            Self::Desc => " ▼",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    List,
    FilterInput,
    LogView,
    SecretDecode,
    ContextSelect,
    NamespaceSelect,
    ScaleInput,
    Confirm,
    ShellView,
    DescribeView,
    StatusFilter,
    LogSearchInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    Pod,
    Deployment,
    Secret,
}

impl ResourceType {
    pub fn index(self) -> usize {
        match self {
            Self::Pod => 0,
            Self::Deployment => 1,
            Self::Secret => 2,
        }
    }

    pub fn sort_column_count(self) -> usize {
        match self {
            Self::Pod => 5,
            Self::Deployment => 5,
            Self::Secret => 4,
        }
    }
}

#[derive(Debug)]
pub struct TaggedWatcherEvent {
    pub tab: ResourceType,
    pub event: KubeResourceEvent,
}

#[derive(Clone, Debug)]
pub enum KubeResource {
    Pod(Arc<Pod>),
    Deployment(Arc<Deployment>),
    Secret(Arc<Secret>),
}

impl KubeResource {
    pub fn name(&self) -> &str {
        let meta = match self {
            KubeResource::Pod(p) => &p.metadata,
            KubeResource::Deployment(d) => &d.metadata,
            KubeResource::Secret(s) => &s.metadata,
        };
        meta.name.as_deref().unwrap_or_default()
    }
}

#[derive(Debug)]
pub enum KubeResourceEvent {
    Refresh,
    InitialListDone,
    Error(String),
    Success(String),
    WatcherForbidden(String),
    Log(String),
    LogHistory(u64, Vec<String>),
    ShellOutput(Vec<u8>),
    ShellExited,
    DescribeReady(Vec<String>),
    NamespacesLoaded(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingAction {
    DeleteResource {
        count: usize,
        kind: &'static str,
        names: Vec<String>,
    },
    RestartDeployment {
        name: String,
    },
    ScaleDeployment {
        name: String,
        replicas: u32,
    },
}

impl PendingAction {
    pub fn message(&self) -> String {
        match self {
            Self::DeleteResource { count, kind, names } => {
                if *count == 1 {
                    format!(
                        "Delete {} '{}'?",
                        kind,
                        names.first().map(|s| s.as_str()).unwrap_or("?")
                    )
                } else {
                    format!("Delete {} {}?\n{}", count, kind, names.join(", "))
                }
            }
            Self::RestartDeployment { name } => {
                format!("Rollout restart '{}'?", name)
            }
            Self::ScaleDeployment { name, replicas } => {
                if *replicas == 0 {
                    format!("Scale '{}' to 0 replicas?\nThis will stop all pods.", name)
                } else {
                    format!("Scale '{}' to {} replicas?", name, replicas)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    fn pod_with_name(name: &str) -> KubeResource {
        let mut pod = Pod::default();
        pod.metadata = ObjectMeta {
            name: Some(name.to_string()),
            ..Default::default()
        };
        KubeResource::Pod(Arc::new(pod))
    }

    fn deployment_with_name(name: &str) -> KubeResource {
        let mut dep = Deployment::default();
        dep.metadata = ObjectMeta {
            name: Some(name.to_string()),
            ..Default::default()
        };
        KubeResource::Deployment(Arc::new(dep))
    }

    fn secret_with_name(name: &str) -> KubeResource {
        let mut secret = Secret::default();
        secret.metadata = ObjectMeta {
            name: Some(name.to_string()),
            ..Default::default()
        };
        KubeResource::Secret(Arc::new(secret))
    }

    #[test]
    fn pod_name() {
        assert_eq!(pod_with_name("nginx").name(), "nginx");
    }

    #[test]
    fn deployment_name() {
        assert_eq!(deployment_with_name("web-app").name(), "web-app");
    }

    #[test]
    fn secret_name() {
        assert_eq!(secret_with_name("db-creds").name(), "db-creds");
    }

    #[test]
    fn empty_metadata_name_returns_empty_str() {
        let pod = Pod::default();
        let res = KubeResource::Pod(Arc::new(pod));
        assert_eq!(res.name(), "");
    }

    #[test]
    fn app_mode_equality() {
        assert_eq!(AppMode::List, AppMode::List);
        assert_ne!(AppMode::List, AppMode::FilterInput);
    }

    #[test]
    fn resource_type_equality() {
        assert_eq!(ResourceType::Pod, ResourceType::Pod);
        assert_ne!(ResourceType::Pod, ResourceType::Secret);
    }

    #[test]
    fn resource_type_index_distinct() {
        let indices = [
            ResourceType::Pod.index(),
            ResourceType::Deployment.index(),
            ResourceType::Secret.index(),
        ];
        assert_eq!(indices[0], 0);
        assert_eq!(indices[1], 1);
        assert_eq!(indices[2], 2);
    }
}
