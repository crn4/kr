use futures::Stream;
use kube::{
    api::{Api, Resource},
    runtime::{reflector, reflector::Store, watcher},
    Client,
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
    let watcher_config = watcher::Config::default().any_semantic().page_size(5000);
    let stream = reflector(writer, watcher(api, watcher_config));
    (reader, stream)
}
