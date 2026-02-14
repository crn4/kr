use anyhow::Result;
use kube::Client;

pub async fn default_client() -> Result<Client> {
    Ok(Client::try_default().await?)
}
