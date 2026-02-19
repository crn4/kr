use anyhow::Result;
use futures::{AsyncBufReadExt, StreamExt};
use k8s_openapi::api::{apps::v1::Deployment, core::v1::Pod};
use kube::Client;
use kube::api::{Api, LogParams};
use tokio::sync::mpsc::UnboundedSender;

use crate::models::KubeResourceEvent;

pub fn stream_pod_logs(
    client: Client,
    namespace: &str,
    pod_name: &str,
    tx: UnboundedSender<KubeResourceEvent>,
    tail_lines: i64,
) -> tokio::task::AbortHandle {
    let namespace = namespace.to_owned();
    let pod_name = pod_name.to_owned();
    let handle = tokio::spawn(async move {
        let pods: Api<Pod> = Api::namespaced(client, &namespace);
        let lp = LogParams {
            follow: true,
            tail_lines: Some(tail_lines),
            ..Default::default()
        };

        match pods.log_stream(&pod_name, &lp).await {
            Ok(stream) => {
                let mut lines = stream.lines();
                while let Some(Ok(line)) = lines.next().await {
                    if tx.send(KubeResourceEvent::Log(line)).is_err() {
                        break;
                    }
                }
            }
            Err(e) => {
                if tx
                    .send(KubeResourceEvent::Error(format!("Log error: {e}")))
                    .is_err()
                {
                    tracing::warn!("Failed to send log error event");
                }
            }
        }
    });
    handle.abort_handle()
}

pub async fn delete_pod(client: Client, namespace: &str, name: &str) -> Result<()> {
    let pods: Api<Pod> = Api::namespaced(client, namespace);
    pods.delete(name, &Default::default()).await?;
    Ok(())
}

pub async fn delete_deployment(client: Client, namespace: &str, name: &str) -> Result<()> {
    let deployments: Api<Deployment> = Api::namespaced(client, namespace);
    deployments.delete(name, &Default::default()).await?;
    Ok(())
}

pub async fn scale_deployment(
    client: Client,
    namespace: &str,
    name: &str,
    replicas: u32,
) -> Result<()> {
    let deployments: Api<Deployment> = Api::namespaced(client, namespace);
    let patch = serde_json::json!({
        "spec": { "replicas": replicas }
    });
    deployments
        .patch(
            name,
            &kube::api::PatchParams::apply("kr"),
            &kube::api::Patch::Merge(&patch),
        )
        .await?;
    Ok(())
}

pub async fn rollout_restart(client: Client, namespace: &str, name: &str) -> Result<()> {
    let deployments: Api<Deployment> = Api::namespaced(client, namespace);
    let now = jiff::Timestamp::now().to_string();
    let patch = serde_json::json!({
        "spec": {
            "template": {
                "metadata": {
                    "annotations": {
                        "kubectl.kubernetes.io/restartedAt": now
                    }
                }
            }
        }
    });
    deployments
        .patch(
            name,
            &kube::api::PatchParams::apply("kr"),
            &kube::api::Patch::Merge(&patch),
        )
        .await?;
    Ok(())
}

pub fn fetch_log_history(
    client: Client,
    namespace: &str,
    pod_name: &str,
    tail_lines: i64,
    generation: u64,
    tx: UnboundedSender<KubeResourceEvent>,
) -> tokio::task::AbortHandle {
    let namespace = namespace.to_owned();
    let pod_name = pod_name.to_owned();
    let handle = tokio::spawn(async move {
        let pods: Api<Pod> = Api::namespaced(client, &namespace);
        let lp = LogParams {
            follow: false,
            tail_lines: Some(tail_lines),
            ..Default::default()
        };
        match pods.log_stream(&pod_name, &lp).await {
            Ok(stream) => {
                let mut lines = Vec::new();
                let mut reader = stream.lines();
                while let Some(Ok(line)) = reader.next().await {
                    lines.push(line);
                }
                let _ = tx.send(KubeResourceEvent::LogHistory(generation, lines));
            }
            Err(e) => {
                let _ = tx.send(KubeResourceEvent::Error(format!("Log history error: {e}")));
            }
        }
    });
    handle.abort_handle()
}
