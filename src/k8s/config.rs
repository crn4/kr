use anyhow::Result;
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Client, Config};

pub fn list_contexts() -> Result<Vec<String>> {
    let config = Kubeconfig::read()?;
    Ok(config.contexts.into_iter().map(|c| c.name).collect())
}

pub fn get_current_context() -> Result<String> {
    let config = Kubeconfig::read()?;
    Ok(config.current_context.unwrap_or_default())
}

pub fn get_context_namespace() -> Result<Option<String>> {
    let config = Kubeconfig::read()?;
    let ctx_name = config.current_context.as_deref().unwrap_or_default();
    Ok(config
        .contexts
        .iter()
        .find(|c| c.name == ctx_name)
        .and_then(|c| c.context.as_ref())
        .and_then(|c| c.namespace.clone()))
}

pub fn configured_namespace_for_context(context: &str) -> Option<String> {
    Kubeconfig::read().ok().and_then(|config| {
        config
            .contexts
            .iter()
            .find(|c| c.name == context)
            .and_then(|c| c.context.as_ref())
            .and_then(|c| c.namespace.clone())
    })
}

pub async fn create_client_with_context(context: &str) -> Result<Client> {
    let options = KubeConfigOptions {
        context: Some(context.to_string()),
        ..Default::default()
    };
    let config = Config::from_kubeconfig(&options).await?;
    let client = Client::try_from(config)?;
    Ok(client)
}

pub async fn list_contexts_async() -> Option<Vec<String>> {
    tokio::task::spawn_blocking(|| list_contexts().ok())
        .await
        .ok()
        .flatten()
}

pub async fn configured_namespace_for_context_async(context: String) -> Option<String> {
    tokio::task::spawn_blocking(move || configured_namespace_for_context(&context))
        .await
        .ok()
        .flatten()
}
