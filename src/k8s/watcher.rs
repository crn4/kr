use futures::Stream;
use kube::{
    Client,
    api::{Api, Resource},
    runtime::{WatchStreamExt, reflector, reflector::Store, watcher},
};
use serde::de::DeserializeOwned;
use std::fmt::Debug;

pub fn reflect_resources<K>(
    client: Client,
    namespace: &str,
) -> (
    Store<K>,
    impl Stream<Item = Result<watcher::Event<K>, watcher::Error>> + use<K>,
)
where
    K: Resource<Scope = k8s_openapi::NamespaceResourceScope>
        + Clone
        + DeserializeOwned
        + Debug
        + Send
        + 'static,
    K::DynamicType: Default + Eq + std::hash::Hash + Clone,
{
    let api = Api::<K>::namespaced(client, namespace);
    let (reader, writer) = reflector::store();
    let watcher_config = watcher::Config::default().any_semantic().page_size(500);
    let stream = reflector(writer, watcher(api, watcher_config).default_backoff());
    (reader, stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use futures::StreamExt;
    use k8s_openapi::api::core::v1::Pod;
    use std::time::Duration;
    use tower::ServiceBuilder;

    fn failing_client(status: u16) -> Client {
        let service =
            tower::service_fn(move |_req: http::Request<kube::client::Body>| async move {
                Ok::<_, std::convert::Infallible>(
                    http::Response::builder()
                        .status(status)
                        .body(kube::client::Body::from(Bytes::from_static(b"{}")))
                        .unwrap(),
                )
            });
        Client::new(ServiceBuilder::new().service(service), "default")
    }

    async fn next_error_at<S>(stream: &mut S, start: tokio::time::Instant) -> Duration
    where
        S: futures::Stream<Item = Result<watcher::Event<Pod>, watcher::Error>> + Unpin,
    {
        loop {
            match stream.next().await.expect("stream ended") {
                Err(_) => return start.elapsed(),
                Ok(_) => continue,
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn failing_list_backs_off_before_retrying() {
        let (_store, stream) = reflect_resources::<Pod>(failing_client(503), "default");
        futures::pin_mut!(stream);

        let start = tokio::time::Instant::now();
        let first_at = next_error_at(&mut stream, start).await;
        let second_at = next_error_at(&mut stream, start).await;
        let gap = second_at - first_at;

        assert!(
            gap >= Duration::from_millis(800),
            "retry gap was {gap:?}, expected at least the 800ms initial backoff"
        );
    }
}
