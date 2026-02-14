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

pub fn get_context_namespace() -> Result<String> {
    let config = Kubeconfig::read()?;
    let ctx_name = config.current_context.as_deref().unwrap_or_default();
    let ns = config
        .contexts
        .iter()
        .find(|c| c.name == ctx_name)
        .and_then(|c| c.context.as_ref())
        .and_then(|c| c.namespace.clone())
        .unwrap_or_else(|| "default".to_string());
    Ok(ns)
}

pub fn get_namespace_for_context(context: &str) -> String {
    Kubeconfig::read()
        .ok()
        .and_then(|config| {
            config
                .contexts
                .iter()
                .find(|c| c.name == context)
                .and_then(|c| c.context.as_ref())
                .and_then(|c| c.namespace.clone())
        })
        .unwrap_or_else(|| "default".to_string())
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
