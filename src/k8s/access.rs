use futures::StreamExt;
use k8s_openapi::api::authorization::v1::{
    ResourceRule, SelfSubjectRulesReview, SelfSubjectRulesReviewSpec, SubjectRulesReviewStatus,
};
use kube::api::PostParams;
use kube::{Api, Client};

const RELEVANT_RESOURCES: [&str; 3] = ["pods", "deployments", "secrets"];
const PROBE_CONCURRENCY: usize = 32;

pub fn is_access_denied(status: &kube::core::Status) -> bool {
    status.is_forbidden()
        || status.code == 403
        || (status.code == 404 && status.message.contains("access denied"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Relevance {
    Filtered {
        relevant: Vec<String>,
        probed: Vec<String>,
    },
    Unfiltered(Vec<String>),
}

impl Relevance {
    pub fn into_parts(self) -> (Vec<String>, crate::models::NamespaceOrigin) {
        match self {
            Relevance::Filtered { relevant, probed } => {
                (relevant, crate::models::NamespaceOrigin::Probed(probed))
            }
            Relevance::Unfiltered(namespaces) => {
                (namespaces, crate::models::NamespaceOrigin::Unverified)
            }
        }
    }
}

pub fn grants_relevant_access(rules: &[ResourceRule]) -> bool {
    rules.iter().any(|rule| {
        let can_read = rule.verbs.iter().any(|v| v == "list" || v == "*");
        let on_relevant = rule.resources.as_ref().is_some_and(|resources| {
            resources
                .iter()
                .any(|r| r == "*" || RELEVANT_RESOURCES.contains(&r.as_str()))
        });
        can_read && on_relevant
    })
}

pub fn status_is_relevant(status: &SubjectRulesReviewStatus) -> bool {
    status.incomplete || grants_relevant_access(&status.resource_rules)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Verdict {
    Relevant(String),
    Irrelevant(String),
    Unknown,
}

async fn probe_namespace(client: Client, namespace: String) -> Verdict {
    let review = SelfSubjectRulesReview {
        spec: SelfSubjectRulesReviewSpec {
            namespace: Some(namespace.clone()),
        },
        ..Default::default()
    };
    let api: Api<SelfSubjectRulesReview> = Api::all(client);
    match api.create(&PostParams::default(), &review).await {
        Ok(result) => match result.status {
            Some(status) if status_is_relevant(&status) => Verdict::Relevant(namespace),
            Some(_) => Verdict::Irrelevant(namespace),
            None => Verdict::Unknown,
        },
        Err(e) => {
            tracing::debug!("rules review for '{namespace}' failed: {e}");
            Verdict::Unknown
        }
    }
}

fn relevance_from(candidates: Vec<String>, verdicts: Vec<Verdict>) -> Relevance {
    let total = candidates.len();
    let mut relevant = Vec::new();
    let mut probed = Vec::new();
    let mut unanswered = 0;

    for verdict in verdicts {
        match verdict {
            Verdict::Relevant(ns) => {
                probed.push(ns.clone());
                relevant.push(ns);
            }
            Verdict::Irrelevant(ns) => probed.push(ns),
            Verdict::Unknown => unanswered += 1,
        }
    }

    if unanswered > 0 {
        tracing::warn!("{unanswered}/{total} namespaces did not answer the access probe");
    }

    if relevant.is_empty() {
        tracing::warn!(
            "{}/{total} namespaces answered and none was relevant, keeping all candidates",
            probed.len()
        );
        return Relevance::Unfiltered(candidates);
    }

    relevant.sort();
    probed.sort();
    tracing::info!(
        "{}/{total} namespaces relevant for this cluster",
        relevant.len()
    );
    Relevance::Filtered { relevant, probed }
}

pub async fn filter_relevant(client: &Client, candidates: Vec<String>) -> Relevance {
    if candidates.is_empty() {
        return Relevance::Unfiltered(candidates);
    }
    let verdicts: Vec<Verdict> = futures::stream::iter(candidates.clone())
        .map(|ns| probe_namespace(client.clone(), ns))
        .buffer_unordered(PROBE_CONCURRENCY)
        .collect()
        .await;

    relevance_from(candidates, verdicts)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn probe_with(status: u16, body: &'static str) -> Verdict {
        let service =
            tower::service_fn(move |_req: http::Request<kube::client::Body>| async move {
                Ok::<_, std::convert::Infallible>(
                    http::Response::builder()
                        .status(status)
                        .body(kube::client::Body::from(body.as_bytes().to_vec()))
                        .unwrap(),
                )
            });
        let client = Client::new(tower::ServiceBuilder::new().service(service), "default");
        probe_namespace(client, "ns".to_string()).await
    }

    const BASELINE_RULES: &str = r#"{"kind":"SelfSubjectRulesReview","apiVersion":"authorization.k8s.io/v1","status":{"incomplete":false,"resourceRules":[{"verbs":["create"],"resources":["selfsubjectrulesreviews"]}],"nonResourceRules":[]}}"#;
    const POD_LIST_RULES: &str = r#"{"kind":"SelfSubjectRulesReview","apiVersion":"authorization.k8s.io/v1","status":{"incomplete":false,"resourceRules":[{"verbs":["list"],"resources":["pods"]}],"nonResourceRules":[]}}"#;

    #[tokio::test]
    async fn a_transport_failure_is_unknown_not_a_rejection() {
        assert_eq!(
            probe_with(503, r#"{"kind":"Status"}"#).await,
            Verdict::Unknown
        );
    }

    #[tokio::test]
    async fn a_response_without_status_is_unknown() {
        assert_eq!(
            probe_with(
                200,
                r#"{"kind":"SelfSubjectRulesReview","apiVersion":"authorization.k8s.io/v1"}"#
            )
            .await,
            Verdict::Unknown
        );
    }

    #[tokio::test]
    async fn baseline_only_rules_are_a_rejection() {
        assert_eq!(
            probe_with(200, BASELINE_RULES).await,
            Verdict::Irrelevant("ns".to_string())
        );
    }

    #[tokio::test]
    async fn pod_list_access_is_relevant_verdict() {
        assert_eq!(
            probe_with(200, POD_LIST_RULES).await,
            Verdict::Relevant("ns".to_string())
        );
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn an_unanswered_probe_is_not_a_rejection() {
        let relevance = relevance_from(
            names(&["ok-ns", "flaky-ns"]),
            vec![Verdict::Relevant("ok-ns".into()), Verdict::Unknown],
        );

        let Relevance::Filtered { relevant, probed } = relevance else {
            panic!("expected a filtered result");
        };
        assert_eq!(relevant, names(&["ok-ns"]));
        assert_eq!(
            probed,
            names(&["ok-ns"]),
            "a namespace whose probe never answered must not be superseded"
        );
    }

    #[test]
    fn a_rejected_probe_is_superseded() {
        let relevance = relevance_from(
            names(&["ok-ns", "denied-ns"]),
            vec![
                Verdict::Relevant("ok-ns".into()),
                Verdict::Irrelevant("denied-ns".into()),
            ],
        );

        let Relevance::Filtered { relevant, probed } = relevance else {
            panic!("expected a filtered result");
        };
        assert_eq!(relevant, names(&["ok-ns"]));
        assert_eq!(probed, names(&["denied-ns", "ok-ns"]));
    }

    #[test]
    fn every_probe_failing_falls_open_to_the_full_list() {
        let relevance =
            relevance_from(names(&["a", "b"]), vec![Verdict::Unknown, Verdict::Unknown]);

        assert_eq!(relevance, Relevance::Unfiltered(names(&["a", "b"])));
    }

    #[test]
    fn every_probe_rejecting_still_falls_open() {
        let relevance = relevance_from(
            names(&["a", "b"]),
            vec![
                Verdict::Irrelevant("a".into()),
                Verdict::Irrelevant("b".into()),
            ],
        );

        assert_eq!(relevance, Relevance::Unfiltered(names(&["a", "b"])));
    }

    fn rule(resources: &[&str], verbs: &[&str]) -> ResourceRule {
        ResourceRule {
            resources: Some(resources.iter().map(|r| r.to_string()).collect()),
            verbs: verbs.iter().map(|v| v.to_string()).collect(),
            ..Default::default()
        }
    }

    fn baseline() -> Vec<ResourceRule> {
        vec![
            rule(
                &["selfsubjectaccessreviews.authorization.k8s.io"],
                &["create"],
            ),
            rule(
                &["selfsubjectrulesreviews.authorization.k8s.io"],
                &["create"],
            ),
        ]
    }

    fn status_code(code: u16, message: &str) -> kube::core::Status {
        kube::core::Status {
            code,
            message: message.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn plain_403_is_access_denied() {
        assert!(is_access_denied(&status_code(403, "pods is forbidden")));
    }

    #[test]
    fn forbidden_reason_is_access_denied() {
        let status = kube::core::Status {
            reason: "Forbidden".to_string(),
            ..status_code(0, "")
        };
        assert!(is_access_denied(&status));
    }

    #[test]
    fn teleport_404_access_denied_is_access_denied() {
        assert!(is_access_denied(&status_code(
            404,
            "Unable to list \"/v1, Resource=pods\": access denied\n\trole some-role is not found"
        )));
    }

    #[test]
    fn genuine_404_is_not_access_denied() {
        assert!(!is_access_denied(&status_code(
            404,
            "namespaces \"nope\" not found"
        )));
    }

    #[test]
    fn other_codes_are_not_access_denied() {
        assert!(!is_access_denied(&status_code(401, "access denied")));
        assert!(!is_access_denied(&status_code(500, "internal error")));
    }

    #[test]
    fn baseline_rules_are_not_relevant() {
        assert!(!grants_relevant_access(&baseline()));
    }

    #[test]
    fn no_rules_is_not_relevant() {
        assert!(!grants_relevant_access(&[]));
    }

    #[test]
    fn pod_list_access_is_relevant() {
        let mut rules = baseline();
        rules.push(rule(&["pods"], &["get", "list", "watch"]));
        assert!(grants_relevant_access(&rules));
    }

    #[test]
    fn secrets_and_deployments_count_as_relevant() {
        assert!(grants_relevant_access(&[rule(&["secrets"], &["list"])]));
        assert!(grants_relevant_access(&[rule(&["deployments"], &["list"])]));
    }

    #[test]
    fn wildcards_are_relevant() {
        assert!(grants_relevant_access(&[rule(&["*"], &["*"])]));
        assert!(grants_relevant_access(&[rule(&["pods"], &["*"])]));
        assert!(grants_relevant_access(&[rule(&["*"], &["list"])]));
    }

    #[test]
    fn write_only_access_is_not_relevant() {
        assert!(!grants_relevant_access(&[rule(
            &["pods"],
            &["create", "delete"]
        )]));
    }

    #[test]
    fn unrelated_resources_are_not_relevant() {
        assert!(!grants_relevant_access(&[rule(&["configmaps"], &["list"])]));
    }

    fn status(rules: Vec<ResourceRule>, incomplete: bool) -> SubjectRulesReviewStatus {
        SubjectRulesReviewStatus {
            resource_rules: rules,
            incomplete,
            ..Default::default()
        }
    }

    #[test]
    fn incomplete_status_fails_open() {
        assert!(status_is_relevant(&status(baseline(), true)));
        assert!(!status_is_relevant(&status(baseline(), false)));
    }

    #[test]
    fn complete_status_with_access_is_relevant() {
        assert!(status_is_relevant(&status(
            vec![rule(&["pods"], &["list"])],
            false
        )));
    }

    #[test]
    fn relevance_into_parts_carries_probed_scope() {
        let relevant = vec!["a".to_string()];
        let probed = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            Relevance::Filtered {
                relevant: relevant.clone(),
                probed: probed.clone(),
            }
            .into_parts(),
            (
                relevant.clone(),
                crate::models::NamespaceOrigin::Probed(probed)
            )
        );
        assert_eq!(
            Relevance::Unfiltered(relevant.clone()).into_parts(),
            (relevant, crate::models::NamespaceOrigin::Unverified)
        );
    }

    #[test]
    fn rule_without_resources_is_not_relevant() {
        let rule = ResourceRule {
            resources: None,
            verbs: vec!["list".to_string()],
            ..Default::default()
        };
        assert!(!grants_relevant_access(&[rule]));
    }
}
