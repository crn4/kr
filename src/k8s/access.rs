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

async fn is_namespace_relevant(client: Client, namespace: String) -> Option<String> {
    let review = SelfSubjectRulesReview {
        spec: SelfSubjectRulesReviewSpec {
            namespace: Some(namespace.clone()),
        },
        ..Default::default()
    };
    let api: Api<SelfSubjectRulesReview> = Api::all(client);
    match api.create(&PostParams::default(), &review).await {
        Ok(result) => result.status.filter(status_is_relevant).map(|_| namespace),
        Err(e) => {
            tracing::debug!("rules review for '{namespace}' failed: {e}");
            None
        }
    }
}

pub async fn filter_relevant(client: &Client, candidates: Vec<String>) -> Relevance {
    if candidates.is_empty() {
        return Relevance::Unfiltered(candidates);
    }
    let total = candidates.len();
    let mut relevant: Vec<String> = futures::stream::iter(candidates.clone())
        .map(|ns| is_namespace_relevant(client.clone(), ns))
        .buffer_unordered(PROBE_CONCURRENCY)
        .filter_map(std::future::ready)
        .collect()
        .await;

    if relevant.is_empty() {
        tracing::warn!("no namespace passed the access probe, keeping all {total} candidates");
        return Relevance::Unfiltered(candidates);
    }
    relevant.sort();
    tracing::info!(
        "{}/{total} namespaces relevant for this cluster",
        relevant.len()
    );
    Relevance::Filtered {
        relevant,
        probed: candidates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
