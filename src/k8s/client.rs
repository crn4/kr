use anyhow::Result;
use kube::{Client, Config};

pub struct Clients {
    pub api: Client,
    pub log_stream: Client,
}

pub fn from_config(config: Config) -> Result<Clients> {
    let mut streaming = config.clone();
    streaming.read_timeout = None;
    Ok(Clients {
        api: Client::try_from(config)?,
        log_stream: Client::try_from(streaming)?,
    })
}

pub async fn default_clients() -> Result<Clients> {
    from_config(Config::infer().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> Config {
        Config::new("https://example.invalid".parse().unwrap())
    }

    #[test]
    fn api_client_keeps_the_default_read_timeout() {
        assert!(base_config().read_timeout.is_some());
    }

    #[test]
    fn log_stream_config_drops_the_read_timeout() {
        let config = base_config();
        let mut streaming = config.clone();
        streaming.read_timeout = None;

        assert!(config.read_timeout.is_some());
        assert!(streaming.read_timeout.is_none());
    }

    #[tokio::test]
    async fn from_config_builds_both_clients() {
        assert!(from_config(base_config()).is_ok());
    }
}
