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
    LogVisualSelect,
    SecretDecode,
    ContextSelect,
    NamespaceSelect,
    ScaleInput,
    Confirm,
    ShellView,
    DescribeView,
    StatusFilter,
    LogSearchInput,
    Help,
    PortForwardInput,
    PortForwardList,
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

    pub fn noun(self, count: usize) -> &'static str {
        match (self, count) {
            (Self::Pod, 1) => "pod",
            (Self::Pod, _) => "pods",
            (Self::Deployment, 1) => "deployment",
            (Self::Deployment, _) => "deployments",
            (Self::Secret, 1) => "secret",
            (Self::Secret, _) => "secrets",
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
    ShellOutput(u64, Vec<u8>),
    ShellExited(u64),
    DescribeReady(Vec<String>),
    NamespacesLoaded {
        context: String,
        namespaces: Vec<String>,
        origin: NamespaceOrigin,
    },
    TeleportState(crate::k8s::teleport::State),
    PortForwardStopped {
        id: u64,
        error: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamespaceOrigin {
    Listed,
    Probed(Vec<String>),
    Unverified,
}

impl NamespaceOrigin {
    pub fn is_verified(&self) -> bool {
        !matches!(self, NamespaceOrigin::Unverified)
    }

    pub fn supersedes(&self) -> Option<&[String]> {
        match self {
            NamespaceOrigin::Probed(probed) => Some(probed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextEntry<'a> {
    Kubeconfig(&'a str),
    Teleport(&'a str),
    TeleportLogin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortForwardTarget {
    pub pod_name: String,
    pub namespace: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingAction {
    DeleteResource {
        resource: ResourceType,
        names: Vec<String>,
    },
    RestartDeployment {
        names: Vec<String>,
    },
    ScaleDeployment {
        names: Vec<String>,
        replicas: u32,
    },
    PortForward {
        pod_name: String,
        namespace: String,
        local_port: u16,
        remote_port: u16,
    },
}

impl PendingAction {
    pub fn message(&self) -> String {
        match self {
            Self::DeleteResource { resource, names } => match names.as_slice() {
                [name] => format!("Delete {} '{name}'?", resource.noun(1)),
                _ => format!(
                    "Delete {} {}?\n{}",
                    names.len(),
                    resource.noun(names.len()),
                    names.join(", ")
                ),
            },
            Self::RestartDeployment { names } => match names.as_slice() {
                [name] => format!("Rollout restart '{name}'?"),
                _ => format!(
                    "Rollout restart {} deployments?\n{}",
                    names.len(),
                    names.join(", ")
                ),
            },
            Self::ScaleDeployment { names, replicas } => {
                let warning = if *replicas == 0 {
                    "\nThis will stop all pods."
                } else {
                    ""
                };
                match names.as_slice() {
                    [name] => format!("Scale '{name}' to {replicas} replicas?{warning}"),
                    _ => format!(
                        "Scale {} deployments to {replicas} replicas?\n{}{warning}",
                        names.len(),
                        names.join(", ")
                    ),
                }
            }
            Self::PortForward {
                pod_name,
                local_port,
                remote_port,
                ..
            } => {
                format!(
                    "Forward localhost:{} → {}:{}?",
                    local_port, pod_name, remote_port
                )
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
    fn delete_message_single_uses_singular_noun() {
        let msg = PendingAction::DeleteResource {
            resource: ResourceType::Pod,
            names: vec!["nginx".into()],
        }
        .message();
        assert_eq!(msg, "Delete pod 'nginx'?");
    }

    #[test]
    fn delete_message_multi_uses_plural_noun_and_lists() {
        let msg = PendingAction::DeleteResource {
            resource: ResourceType::Deployment,
            names: vec!["web".into(), "api".into()],
        }
        .message();
        assert_eq!(msg, "Delete 2 deployments?\nweb, api");
    }

    #[test]
    fn resource_type_noun_pluralizes() {
        assert_eq!(ResourceType::Pod.noun(1), "pod");
        assert_eq!(ResourceType::Pod.noun(0), "pods");
        assert_eq!(ResourceType::Secret.noun(3), "secrets");
    }

    #[test]
    fn scale_message_single() {
        let msg = PendingAction::ScaleDeployment {
            names: vec!["web".into()],
            replicas: 3,
        }
        .message();
        assert_eq!(msg, "Scale 'web' to 3 replicas?");
    }

    #[test]
    fn scale_message_single_zero_warns() {
        let msg = PendingAction::ScaleDeployment {
            names: vec!["web".into()],
            replicas: 0,
        }
        .message();
        assert!(msg.starts_with("Scale 'web' to 0 replicas?"));
        assert!(msg.contains("This will stop all pods."));
    }

    #[test]
    fn scale_message_multi_lists_names() {
        let msg = PendingAction::ScaleDeployment {
            names: vec!["web".into(), "api".into()],
            replicas: 2,
        }
        .message();
        assert_eq!(msg, "Scale 2 deployments to 2 replicas?\nweb, api");
    }

    #[test]
    fn scale_message_multi_zero_warns_and_lists() {
        let msg = PendingAction::ScaleDeployment {
            names: vec!["web".into(), "api".into()],
            replicas: 0,
        }
        .message();
        assert_eq!(
            msg,
            "Scale 2 deployments to 0 replicas?\nweb, api\nThis will stop all pods."
        );
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
