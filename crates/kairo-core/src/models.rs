use std::collections::BTreeMap;

use chrono::Utc;
use serde::Serialize;
use k8s_openapi::api::apps::v1::{Deployment as K8sDeployment, ReplicaSet as K8sReplicaSet};
use k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscaler as K8sHpa;
use k8s_openapi::api::core::v1::{
    ConfigMap as K8sConfigMap, ContainerState, ContainerStatus as K8sContainerStatus,
    Event as K8sEvent, Node as K8sNode, Pod, Service as K8sService,
};
use k8s_openapi::api::networking::v1::Ingress as K8sIngress;
use k8s_openapi::api::storage::v1::StorageClass as K8sStorageClass;

// ── Relationship types ────────────────────────────────────────────────────────

/// Scope of a Kubernetes resource.
#[derive(Debug, Clone, PartialEq)]
pub enum ResourceScope {
    Cluster,
    Namespaced,
    Embedded,
}

/// Directed relationship type between two resources.
#[derive(Debug, Clone, PartialEq)]
pub enum RelationType {
    References,
    TargetedBy,
    Selects,
    RoutesTo,
    BindsTo,
    UsesStorageClass,
    Targets,
}

/// A directed relationship from one tree node to another resource.
#[derive(Debug, Clone)]
pub struct ResourceRelationship {
    pub rel_type: RelationType,
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
}

// ── PodSummary ────────────────────────────────────────────────────────────────

/// Summary of a Pod for list views.
#[derive(Debug, Clone)]
pub struct PodSummary {
    /// Pod name.
    pub name: String,
    /// Pod namespace.
    pub namespace: String,
    /// Human-readable status (phase, CrashLoopBackOff, Terminating, …).
    pub status: String,
    /// Ready containers as "n/m".
    pub ready: String,
    /// Total restart count across all containers.
    pub restarts: i32,
    /// Human-readable age since creation (e.g. "5d", "3h", "12m").
    pub age: String,
    /// Node the pod is scheduled on.
    pub node: String,
    /// Pod labels, used for client-side label-selector filtering.
    pub labels: BTreeMap<String, String>,
    /// Direct owner kind (ReplicaSet, DaemonSet, StatefulSet, Job, …). Empty if standalone.
    pub owner_kind: String,
    /// Name of the owning object. Empty if standalone.
    pub owner_name: String,
    /// Names of containers defined in spec (for tree Container leaf nodes).
    pub container_names: Vec<String>,
    /// Resources referenced by this pod (SA, ConfigMaps, Secrets, PVCs from spec).
    pub references: Vec<ResourceRelationship>,
}

impl From<Pod> for PodSummary {
    fn from(pod: Pod) -> Self {
        let name = pod.metadata.name.clone().unwrap_or_default();
        let namespace = pod.metadata.namespace.clone().unwrap_or_default();
        let labels = pod.metadata.labels.clone().unwrap_or_default();

        let (owner_kind, owner_name) = pod
            .metadata
            .owner_references
            .as_deref()
            .and_then(|refs| refs.first())
            .map(|r| (r.kind.clone(), r.name.clone()))
            .unwrap_or_default();

        let container_statuses = pod
            .status
            .as_ref()
            .and_then(|s| s.container_statuses.as_deref())
            .unwrap_or(&[]);

        let ready_count = container_statuses.iter().filter(|cs| cs.ready).count();
        let total_count = container_statuses.len();
        let ready = format!("{}/{}", ready_count, total_count);

        let restarts = container_statuses
            .iter()
            .map(|cs| cs.restart_count)
            .sum();

        let status = pod_status_string(&pod);
        let age = human_age(&pod);
        let node = pod
            .spec
            .as_ref()
            .and_then(|s| s.node_name.clone())
            .unwrap_or_default();

        let container_names = pod.spec.as_ref()
            .map(|s| s.containers.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default();
        let references = extract_pod_references(&pod);

        PodSummary { name, namespace, status, ready, restarts, age, node, labels, owner_kind, owner_name, container_names, references }
    }
}

/// Extract cross-resource references from a pod spec (SA, ConfigMaps, Secrets, PVCs).
fn extract_pod_references(pod: &Pod) -> Vec<ResourceRelationship> {
    let mut refs = Vec::new();
    let Some(spec) = pod.spec.as_ref() else { return refs };

    if let Some(sa) = spec.service_account_name.as_deref() {
        if !sa.is_empty() && sa != "default" {
            refs.push(ResourceRelationship { rel_type: RelationType::References, kind: "ServiceAccount".to_string(), name: sa.to_string(), namespace: None });
        }
    }

    for vol in spec.volumes.as_deref().unwrap_or(&[]) {
        if let Some(cm) = &vol.config_map {
            let n = cm.name.trim().to_string();
            if !n.is_empty() {
                refs.push(ResourceRelationship { rel_type: RelationType::References, kind: "ConfigMap".to_string(), name: n, namespace: None });
            }
        }
        if let Some(secret) = &vol.secret {
            if let Some(n) = secret.secret_name.as_deref() {
                if !n.is_empty() {
                    refs.push(ResourceRelationship { rel_type: RelationType::References, kind: "Secret".to_string(), name: n.to_string(), namespace: None });
                }
            }
        }
        if let Some(pvc) = &vol.persistent_volume_claim {
            refs.push(ResourceRelationship { rel_type: RelationType::References, kind: "PersistentVolumeClaim".to_string(), name: pvc.claim_name.clone(), namespace: None });
        }
    }

    // Dedup by kind + name
    let mut seen = std::collections::HashSet::new();
    refs.retain(|r| seen.insert((r.kind.clone(), r.name.clone())));
    refs
}

/// Derive the display status string for a pod.
fn pod_status_string(pod: &Pod) -> String {
    // Terminating trumps everything else.
    if pod.metadata.deletion_timestamp.is_some() {
        return "Terminating".to_string();
    }

    // If any container is in a named waiting state, surface that.
    if let Some(statuses) = pod
        .status
        .as_ref()
        .and_then(|s| s.container_statuses.as_deref())
    {
        for cs in statuses {
            if let Some(reason) = waiting_reason(&cs.state) {
                return reason;
            }
        }
    }

    // Fall back to the pod phase.
    pod.status
        .as_ref()
        .and_then(|s| s.phase.clone())
        .unwrap_or_else(|| "Unknown".to_string())
}

/// Extract the waiting reason from a container state, if present.
fn waiting_reason(state: &Option<ContainerState>) -> Option<String> {
    state
        .as_ref()?
        .waiting
        .as_ref()?
        .reason
        .clone()
}

/// Produce a human-readable age string from the pod's creation timestamp.
fn human_age(pod: &Pod) -> String {
    // k8s-openapi 0.27 uses jiff::Timestamp; extract Unix epoch seconds via as_second().
    match pod.metadata.creation_timestamp.as_ref() {
        Some(t) => fmt_age_secs(Utc::now().timestamp() - t.0.as_second()),
        None => "?".to_string(),
    }
}

// ── ContainerStatus ───────────────────────────────────────────────────────────

/// Status of a single container within a Pod.
#[derive(Debug, Clone)]
pub struct ContainerStatus {
    /// Container name.
    pub name: String,
    /// Container image reference.
    pub image: String,
    /// Whether the container has passed its readiness probe.
    pub ready: bool,
    /// Number of times the container has restarted.
    pub restart_count: i32,
    /// Human-readable state ("Running", "Waiting", reason, or "Unknown").
    pub state: String,
}

impl From<K8sContainerStatus> for ContainerStatus {
    fn from(cs: K8sContainerStatus) -> Self {
        let state = container_state_string(&cs.state);
        ContainerStatus {
            name: cs.name,
            image: cs.image,
            ready: cs.ready,
            restart_count: cs.restart_count,
            state,
        }
    }
}

/// Summarise a `ContainerState` as a display string.
fn container_state_string(state: &Option<ContainerState>) -> String {
    let Some(s) = state else {
        return "Unknown".to_string();
    };
    if s.running.is_some() {
        return "Running".to_string();
    }
    if let Some(w) = &s.waiting {
        return w.reason.clone().unwrap_or_else(|| "Waiting".to_string());
    }
    if let Some(t) = &s.terminated {
        return t.reason.clone().unwrap_or_else(|| "Terminated".to_string());
    }
    "Unknown".to_string()
}

// ── PodEvent ──────────────────────────────────────────────────────────────────

/// A Kubernetes event related to a Pod.
#[derive(Debug, Clone, Serialize)]
pub struct PodEvent {
    /// Short, CamelCase reason (e.g. "BackOff").
    pub reason: String,
    /// Human-readable event message.
    pub message: String,
    /// "Normal" or "Warning".
    pub event_type: String,
    /// How many times this event has occurred.
    pub count: i32,
    /// RFC 3339 timestamp of the first occurrence.
    pub first_time: String,
    /// RFC 3339 timestamp of the most recent occurrence.
    pub last_time: String,
}

impl From<K8sEvent> for PodEvent {
    fn from(ev: K8sEvent) -> Self {
        // jiff::Timestamp's Display impl outputs ISO 8601 / RFC 3339.
        let first_time = ev
            .first_timestamp
            .as_ref()
            .map(|t| t.0.to_string())
            .unwrap_or_default();
        let last_time = ev
            .last_timestamp
            .as_ref()
            .map(|t| t.0.to_string())
            .unwrap_or_default();

        PodEvent {
            reason: ev.reason.unwrap_or_default(),
            message: ev.message.unwrap_or_default(),
            event_type: ev.type_.unwrap_or_default(),
            count: ev.count.unwrap_or(0),
            first_time,
            last_time,
        }
    }
}

// ── PodDetail ─────────────────────────────────────────────────────────────────

/// Detailed Pod information for the detail panel.
#[derive(Debug, Clone)]
pub struct PodDetail {
    /// High-level summary (shared with the list view).
    pub summary: PodSummary,
    /// Pod labels.
    pub labels: BTreeMap<String, String>,
    /// Pod annotations.
    pub annotations: BTreeMap<String, String>,
    /// Per-container statuses.
    pub containers: Vec<ContainerStatus>,
    /// Related events — populated externally after a separate Event API call.
    pub events: Vec<PodEvent>,
}

impl From<Pod> for PodDetail {
    fn from(pod: Pod) -> Self {
        let labels = pod
            .metadata
            .labels
            .clone()
            .unwrap_or_default();
        let annotations = pod
            .metadata
            .annotations
            .clone()
            .unwrap_or_default();
        let containers = pod
            .status
            .as_ref()
            .and_then(|s| s.container_statuses.clone())
            .unwrap_or_default()
            .into_iter()
            .map(ContainerStatus::from)
            .collect();

        let summary = PodSummary::from(pod);

        PodDetail { summary, labels, annotations, containers, events: vec![] }
    }
}

// ── ClusterEvent ─────────────────────────────────────────────────────────────

/// A Kubernetes cluster-level event (used for warning aggregation in the sidebar).
#[derive(Debug, Clone, Serialize)]
pub struct ClusterEvent {
    /// Namespace where the event originated.
    pub namespace: String,
    /// Kind of the involved object (Pod, Deployment, Service, …).
    pub object_kind: String,
    /// Name of the involved object.
    pub object_name: String,
    /// Short, CamelCase reason (e.g. "BackOff", "Failed").
    pub reason: String,
    /// Human-readable event message.
    pub message: String,
    /// "Normal" or "Warning".
    pub event_type: String,
    /// How many times this event has occurred.
    pub count: i32,
    /// RFC 3339 timestamp of the most recent occurrence.
    pub last_time: String,
}

impl From<K8sEvent> for ClusterEvent {
    fn from(ev: K8sEvent) -> Self {
        let last_time = ev
            .last_timestamp
            .as_ref()
            .map(|t| t.0.to_string())
            .unwrap_or_default();
        let object_kind = ev.involved_object.kind.clone().unwrap_or_default();
        let object_name = ev.involved_object.name.clone().unwrap_or_default();
        let namespace = ev.metadata.namespace.clone().unwrap_or_default();
        ClusterEvent {
            namespace,
            object_kind,
            object_name,
            reason: ev.reason.unwrap_or_default(),
            message: ev.message.unwrap_or_default(),
            event_type: ev.type_.unwrap_or_default(),
            count: ev.count.unwrap_or(0),
            last_time,
        }
    }
}

// ── GenericResourceDetail ────────────────────────────────────────────────────

/// Minimal detail view for any Kubernetes resource kind — used as the fallback
/// when no typed renderer exists (ReplicaSet, Job, Ingress, PVC, HPA, …) or
/// when a typed fetch has failed (resource deleted / forbidden).
#[derive(Debug, Clone)]
pub struct GenericResourceDetail {
    pub kind: String,
    /// Empty string for cluster-scoped resources.
    pub namespace: String,
    pub name: String,
    pub age: Option<String>,
    /// One-line human-readable status (e.g. "Active", "3/3 ready", "Complete").
    pub status_summary: Option<String>,
    /// Human-readable error. When `Some`, the renderer surfaces a "resource
    /// unavailable" card instead of the normal fields.
    pub error: Option<String>,
    /// True for the placeholder shown before the async fetch lands.
    pub loading: bool,
}

impl GenericResourceDetail {
    /// Placeholder shown immediately on click, before the YAML fetch lands.
    pub fn loading(kind: &str, namespace: &str, name: &str) -> Self {
        Self {
            kind: kind.to_string(),
            namespace: namespace.to_string(),
            name: name.to_string(),
            age: None,
            status_summary: None,
            error: None,
            loading: true,
        }
    }

    /// Unavailable placeholder — shown when the fetch errored out.
    pub fn unavailable(kind: &str, namespace: &str, name: &str, reason: String) -> Self {
        Self {
            kind: kind.to_string(),
            namespace: namespace.to_string(),
            name: name.to_string(),
            age: None,
            status_summary: None,
            error: Some(reason),
            loading: false,
        }
    }

    /// Populate from a parsed JSON value (typically serde_yaml → serde_json value
    /// of a DynamicObject manifest). Extracts `metadata.creationTimestamp` → age
    /// and derives a kind-appropriate `status_summary` from `.status`.
    pub fn from_manifest(kind: &str, namespace: &str, name: &str, manifest: &serde_json::Value) -> Self {
        let age = manifest
            .get("metadata")
            .and_then(|m| m.get("creationTimestamp"))
            .and_then(|t| t.as_str())
            .map(age_from_rfc3339);
        let status_summary = status_summary_from_manifest(kind, manifest);

        Self {
            kind: kind.to_string(),
            namespace: namespace.to_string(),
            name: name.to_string(),
            age,
            status_summary,
            error: None,
            loading: false,
        }
    }
}

/// Derive a one-line status summary from a resource manifest's `.status` field.
///
/// Handles common kinds inline; falls back to `status.phase`, a single `Ready`
/// condition, or `None` when nothing useful is present.
fn status_summary_from_manifest(kind: &str, v: &serde_json::Value) -> Option<String> {
    let status = v.get("status")?;

    match kind {
        "ReplicaSet" | "StatefulSet" | "DaemonSet" => {
            let ready = status.get("readyReplicas").and_then(|x| x.as_i64()).unwrap_or(0);
            let desired = v.get("spec")
                .and_then(|s| s.get("replicas"))
                .and_then(|x| x.as_i64())
                .or_else(|| status.get("replicas").and_then(|x| x.as_i64()))
                .unwrap_or(0);
            Some(format!("{ready}/{desired} ready"))
        }
        "Job" => {
            let succeeded = status.get("succeeded").and_then(|x| x.as_i64()).unwrap_or(0);
            let failed = status.get("failed").and_then(|x| x.as_i64()).unwrap_or(0);
            let active = status.get("active").and_then(|x| x.as_i64()).unwrap_or(0);
            if succeeded > 0 && active == 0 && failed == 0 {
                Some("Complete".to_string())
            } else if failed > 0 {
                Some(format!("Failed ({failed})"))
            } else if active > 0 {
                Some(format!("Active ({active})"))
            } else {
                None
            }
        }
        "CronJob" => {
            let active = status.get("active").and_then(|a| a.as_array()).map(|a| a.len()).unwrap_or(0);
            let last = status.get("lastScheduleTime").and_then(|t| t.as_str()).unwrap_or("-");
            Some(format!("active: {active} | last: {last}"))
        }
        "PersistentVolumeClaim" | "PersistentVolume" => {
            status.get("phase").and_then(|p| p.as_str()).map(String::from)
        }
        "Ingress" => {
            let ingress = status.get("loadBalancer").and_then(|lb| lb.get("ingress")).and_then(|i| i.as_array());
            match ingress {
                Some(list) if !list.is_empty() => {
                    let addrs: Vec<String> = list
                        .iter()
                        .filter_map(|e| e.get("hostname").and_then(|h| h.as_str())
                            .or_else(|| e.get("ip").and_then(|i| i.as_str()))
                            .map(String::from))
                        .collect();
                    Some(addrs.join(", "))
                }
                _ => Some("Pending address".to_string()),
            }
        }
        _ => {
            // Generic fallback: status.phase, or the first condition of type=Ready.
            if let Some(phase) = status.get("phase").and_then(|p| p.as_str()) {
                return Some(phase.to_string());
            }
            let conditions = status.get("conditions").and_then(|c| c.as_array())?;
            let ready = conditions.iter().find(|c| {
                c.get("type").and_then(|t| t.as_str()) == Some("Ready")
            })?;
            let is_true = ready.get("status").and_then(|s| s.as_str()) == Some("True");
            Some(if is_true { "Ready".to_string() } else { "NotReady".to_string() })
        }
    }
}

/// Convert an RFC 3339 timestamp string to a human age string ("5d", "3h", …).
fn age_from_rfc3339(ts: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(ts) {
        Ok(dt) => fmt_age_secs(Utc::now().timestamp() - dt.timestamp()),
        Err(_) => "?".to_string(),
    }
}

// ── Shared age helper ────────────────────────────────────────────────────────

/// Convert an optional k8s `Time` into a human-readable age string.
fn age_from_ts(ts: Option<&k8s_openapi::apimachinery::pkg::apis::meta::v1::Time>) -> String {
    match ts {
        Some(t) => fmt_age_secs(Utc::now().timestamp() - t.0.as_second()),
        None => "?".to_string(),
    }
}

/// Format a non-negative elapsed-seconds count as a compact age ("5d", "3h", …).
fn fmt_age_secs(secs: i64) -> String {
    let secs = secs.max(0);
    if secs < 60 { format!("{}s", secs) }
    else if secs < 3600 { format!("{}m", secs / 60) }
    else if secs < 86400 { format!("{}h", secs / 3600) }
    else { format!("{}d", secs / 86400) }
}

// ── DeploymentSummary ─────────────────────────────────────────────────────────

/// Summary of a Deployment for list views.
#[derive(Debug, Clone)]
pub struct DeploymentSummary {
    pub name: String,
    pub namespace: String,
    /// "ready/desired" string, e.g. "2/3".
    pub ready: String,
    pub up_to_date: i32,
    pub available: i32,
    pub age: String,
}

impl From<K8sDeployment> for DeploymentSummary {
    fn from(d: K8sDeployment) -> Self {
        let name = d.metadata.name.unwrap_or_default();
        let namespace = d.metadata.namespace.unwrap_or_default();
        let desired = d.spec.as_ref().and_then(|s| s.replicas).unwrap_or(0);
        let ready_r = d.status.as_ref().and_then(|s| s.ready_replicas).unwrap_or(0);
        let up_to_date = d.status.as_ref().and_then(|s| s.updated_replicas).unwrap_or(0);
        let available = d.status.as_ref().and_then(|s| s.available_replicas).unwrap_or(0);
        DeploymentSummary {
            name,
            namespace,
            ready: format!("{}/{}", ready_r, desired),
            up_to_date,
            available,
            age: age_from_ts(d.metadata.creation_timestamp.as_ref()),
        }
    }
}

// ── ServiceSummary ────────────────────────────────────────────────────────────

/// Summary of a Service for list views.
#[derive(Debug, Clone)]
pub struct ServiceSummary {
    pub name: String,
    pub namespace: String,
    /// Service type: ClusterIP, NodePort, LoadBalancer, ExternalName.
    pub type_: String,
    pub cluster_ip: String,
    /// LoadBalancer ingress IP/hostname, or "<none>".
    pub external_ip: String,
    /// Formatted port list, e.g. "80/TCP,443:30443/TCP".
    pub ports: String,
    pub age: String,
    /// Label selector used to match pods (for Selects relationship in tree).
    pub selector: BTreeMap<String, String>,
}

impl From<K8sService> for ServiceSummary {
    fn from(svc: K8sService) -> Self {
        let name = svc.metadata.name.unwrap_or_default();
        let namespace = svc.metadata.namespace.unwrap_or_default();
        let spec = svc.spec.as_ref();
        let type_ = spec.and_then(|s| s.type_.clone()).unwrap_or_else(|| "ClusterIP".to_string());
        let cluster_ip = spec.and_then(|s| s.cluster_ip.clone()).unwrap_or_else(|| "<none>".to_string());
        let external_ip = svc.status.as_ref()
            .and_then(|s| s.load_balancer.as_ref())
            .and_then(|lb| lb.ingress.as_deref())
            .and_then(|ing| ing.first())
            .and_then(|i| i.ip.clone().or_else(|| i.hostname.clone()))
            .unwrap_or_else(|| "<none>".to_string());
        let ports = spec.and_then(|s| s.ports.as_deref())
            .map(|ps| ps.iter().map(|p| {
                let proto = p.protocol.as_deref().unwrap_or("TCP");
                match p.node_port {
                    Some(np) => format!("{}:{}/{}", p.port, np, proto),
                    None => format!("{}/{}", p.port, proto),
                }
            }).collect::<Vec<_>>().join(","))
            .unwrap_or_else(|| "<none>".to_string());
        let selector = spec.and_then(|s| s.selector.clone()).unwrap_or_default();
        ServiceSummary { name, namespace, type_, cluster_ip, external_ip, ports, selector,
            age: age_from_ts(svc.metadata.creation_timestamp.as_ref()) }
    }
}

// ── ConfigMapSummary ──────────────────────────────────────────────────────────

/// Summary of a ConfigMap for list views.
#[derive(Debug, Clone)]
pub struct ConfigMapSummary {
    pub name: String,
    pub namespace: String,
    /// Number of keys in `data` + `binary_data`.
    pub data_count: usize,
    pub age: String,
}

impl From<K8sConfigMap> for ConfigMapSummary {
    fn from(cm: K8sConfigMap) -> Self {
        let data_count = cm.data.as_ref().map(|d| d.len()).unwrap_or(0)
            + cm.binary_data.as_ref().map(|d| d.len()).unwrap_or(0);
        ConfigMapSummary {
            name: cm.metadata.name.unwrap_or_default(),
            namespace: cm.metadata.namespace.unwrap_or_default(),
            data_count,
            age: age_from_ts(cm.metadata.creation_timestamp.as_ref()),
        }
    }
}

// ── NodeSummary ───────────────────────────────────────────────────────────────

/// Summary of a Node for list views.
#[derive(Debug, Clone)]
pub struct NodeSummary {
    pub name: String,
    /// "Ready", "NotReady", or "Unknown".
    pub status: String,
    /// Comma-separated roles from `node-role.kubernetes.io/<role>` labels.
    pub roles: String,
    pub age: String,
    /// Kubelet version string.
    pub version: String,
    pub os_image: String,
    /// Total CPU capacity in millicores (0 if unavailable).
    pub cpu_capacity_milli: i64,
    /// Allocatable CPU in millicores (schedulable by pods).
    pub cpu_allocatable_milli: i64,
    /// Total memory capacity in bytes (0 if unavailable).
    pub memory_capacity_bytes: i64,
    /// Allocatable memory in bytes.
    pub memory_allocatable_bytes: i64,
    /// Maximum pod count capacity (0 if unavailable).
    pub pod_capacity: i64,
}

impl From<K8sNode> for NodeSummary {
    fn from(node: K8sNode) -> Self {
        let name = node.metadata.name.clone().unwrap_or_default();
        let roles = node.metadata.labels.as_ref()
            .map(|lbls| {
                let mut r: Vec<&str> = lbls.keys()
                    .filter_map(|k| k.strip_prefix("node-role.kubernetes.io/"))
                    .collect();
                r.sort();
                if r.is_empty() { "<none>".to_string() } else { r.join(",") }
            })
            .unwrap_or_else(|| "<none>".to_string());
        let status = node.status.as_ref()
            .and_then(|s| s.conditions.as_deref())
            .and_then(|cs| cs.iter().find(|c| c.type_ == "Ready"))
            .map(|c| if c.status == "True" { "Ready" } else { "NotReady" })
            .unwrap_or("Unknown")
            .to_string();
        let version = node.status.as_ref()
            .and_then(|s| s.node_info.as_ref())
            .map(|i| i.kubelet_version.clone())
            .unwrap_or_default();
        let os_image = node.status.as_ref()
            .and_then(|s| s.node_info.as_ref())
            .map(|i| i.os_image.clone())
            .unwrap_or_default();

        let capacity = node.status.as_ref().and_then(|s| s.capacity.as_ref());
        let allocatable = node.status.as_ref().and_then(|s| s.allocatable.as_ref());

        let cpu_capacity_milli = capacity
            .and_then(|c| c.get("cpu"))
            .map(|q| parse_cpu_millis(&q.0))
            .unwrap_or(0);
        let cpu_allocatable_milli = allocatable
            .and_then(|c| c.get("cpu"))
            .map(|q| parse_cpu_millis(&q.0))
            .unwrap_or(0);
        let memory_capacity_bytes = capacity
            .and_then(|c| c.get("memory"))
            .map(|q| parse_memory_bytes(&q.0))
            .unwrap_or(0);
        let memory_allocatable_bytes = allocatable
            .and_then(|c| c.get("memory"))
            .map(|q| parse_memory_bytes(&q.0))
            .unwrap_or(0);
        let pod_capacity = capacity
            .and_then(|c| c.get("pods"))
            .and_then(|q| q.0.parse::<i64>().ok())
            .unwrap_or(0);

        NodeSummary {
            name, status, roles, version, os_image,
            age: age_from_ts(node.metadata.creation_timestamp.as_ref()),
            cpu_capacity_milli,
            cpu_allocatable_milli,
            memory_capacity_bytes,
            memory_allocatable_bytes,
            pod_capacity,
        }
    }
}

// ── Kubernetes quantity parsers ───────────────────────────────────────────────

/// Parse a Kubernetes CPU quantity string into millicores.
///
/// Examples: `"4"` → 4000, `"500m"` → 500, `"2500m"` → 2500.
pub fn parse_cpu_millis(s: &str) -> i64 {
    if let Some(cores) = s.strip_suffix('m') {
        cores.parse::<i64>().unwrap_or(0)
    } else {
        (s.parse::<f64>().unwrap_or(0.0) * 1000.0) as i64
    }
}

/// Parse a Kubernetes memory quantity string into bytes.
///
/// Examples: `"8Gi"` → 8×2³⁰, `"512Mi"` → 512×2²⁰, `"1G"` → 10⁹.
pub fn parse_memory_bytes(s: &str) -> i64 {
    if let Some(n) = s.strip_suffix("Ki") {
        n.parse::<i64>().unwrap_or(0) * 1024
    } else if let Some(n) = s.strip_suffix("Mi") {
        n.parse::<i64>().unwrap_or(0) * 1024 * 1024
    } else if let Some(n) = s.strip_suffix("Gi") {
        n.parse::<i64>().unwrap_or(0) * 1024 * 1024 * 1024
    } else if let Some(n) = s.strip_suffix("Ti") {
        n.parse::<i64>().unwrap_or(0) * 1024 * 1024 * 1024 * 1024
    } else if let Some(n) = s.strip_suffix('k') {
        n.parse::<i64>().unwrap_or(0) * 1000
    } else if let Some(n) = s.strip_suffix('M') {
        n.parse::<i64>().unwrap_or(0) * 1_000_000
    } else if let Some(n) = s.strip_suffix('G') {
        n.parse::<i64>().unwrap_or(0) * 1_000_000_000
    } else if let Some(n) = s.strip_suffix('T') {
        n.parse::<i64>().unwrap_or(0) * 1_000_000_000_000
    } else {
        s.parse::<i64>().unwrap_or(0)
    }
}

/// Format millicores as a human-readable CPU string.
pub fn fmt_cpu(millis: i64) -> String {
    if millis == 0 { return "?".to_string(); }
    if millis % 1000 == 0 { format!("{}", millis / 1000) }
    else { format!("{}m", millis) }
}

/// Format bytes as a human-readable memory string.
pub fn fmt_memory(bytes: i64) -> String {
    const GI: i64 = 1024 * 1024 * 1024;
    const MI: i64 = 1024 * 1024;
    if bytes == 0 { return "?".to_string(); }
    if bytes >= GI { format!("{:.1}Gi", bytes as f64 / GI as f64) }
    else if bytes >= MI { format!("{:.0}Mi", bytes as f64 / MI as f64) }
    else { format!("{} Ki", bytes / 1024) }
}

// ── ReplicaSetSummary ─────────────────────────────────────────────────────────

/// Summary of a ReplicaSet for tree hierarchy (Deployment → RS → Pod).
#[derive(Debug, Clone)]
pub struct ReplicaSetSummary {
    pub name: String,
    pub namespace: String,
    /// Name of the owning Deployment, if any.
    pub owner_deployment: Option<String>,
    pub ready: i32,
    pub desired: i32,
    pub age: String,
}

impl From<K8sReplicaSet> for ReplicaSetSummary {
    fn from(rs: K8sReplicaSet) -> Self {
        let name = rs.metadata.name.clone().unwrap_or_default();
        let namespace = rs.metadata.namespace.clone().unwrap_or_default();
        let owner_deployment = rs.metadata.owner_references.as_deref()
            .and_then(|refs| refs.iter().find(|r| r.kind == "Deployment"))
            .map(|r| r.name.clone());
        let desired = rs.spec.as_ref().and_then(|s| s.replicas).unwrap_or(0);
        let ready = rs.status.as_ref().and_then(|s| s.ready_replicas).unwrap_or(0);
        ReplicaSetSummary {
            name,
            namespace,
            owner_deployment,
            ready,
            desired,
            age: age_from_ts(rs.metadata.creation_timestamp.as_ref()),
        }
    }
}

// ── IngressSummary ────────────────────────────────────────────────────────────

/// Summary of an Ingress resource, with backend service names for relationship wiring.
#[derive(Debug, Clone)]
pub struct IngressSummary {
    pub name: String,
    pub namespace: String,
    /// Names of backend Services this ingress routes to.
    pub backend_services: Vec<String>,
    pub age: String,
}

impl From<K8sIngress> for IngressSummary {
    fn from(ing: K8sIngress) -> Self {
        let name = ing.metadata.name.clone().unwrap_or_default();
        let namespace = ing.metadata.namespace.clone().unwrap_or_default();
        let mut backend_services: Vec<String> = Vec::new();

        if let Some(spec) = &ing.spec {
            // Default backend
            if let Some(default_be) = &spec.default_backend {
                if let Some(svc) = &default_be.service {
                    backend_services.push(svc.name.clone());
                }
            }
            // Rule backends
            for rule in spec.rules.as_deref().unwrap_or(&[]) {
                if let Some(http) = &rule.http {
                    for path in &http.paths {
                        if let Some(svc) = &path.backend.service {
                            backend_services.push(svc.name.clone());
                        }
                    }
                }
            }
        }
        backend_services.sort();
        backend_services.dedup();
        IngressSummary { name, namespace, backend_services, age: age_from_ts(ing.metadata.creation_timestamp.as_ref()) }
    }
}

// ── HpaSummary ────────────────────────────────────────────────────────────────

/// Summary of a HorizontalPodAutoscaler for relationship wiring.
#[derive(Debug, Clone)]
pub struct HpaSummary {
    pub name: String,
    pub namespace: String,
    pub target_kind: String,
    pub target_name: String,
    pub min_replicas: i32,
    pub max_replicas: i32,
    pub current_replicas: i32,
    pub age: String,
}

impl From<K8sHpa> for HpaSummary {
    fn from(hpa: K8sHpa) -> Self {
        let name = hpa.metadata.name.clone().unwrap_or_default();
        let namespace = hpa.metadata.namespace.clone().unwrap_or_default();
        let (target_kind, target_name, min_replicas, max_replicas) = hpa.spec.as_ref()
            .map(|s| (
                s.scale_target_ref.kind.clone(),
                s.scale_target_ref.name.clone(),
                s.min_replicas.unwrap_or(1),
                s.max_replicas,
            ))
            .unwrap_or_default();
        let current_replicas = hpa.status.as_ref().and_then(|s| s.current_replicas).unwrap_or(0);
        HpaSummary {
            name, namespace, target_kind, target_name,
            min_replicas, max_replicas, current_replicas,
            age: age_from_ts(hpa.metadata.creation_timestamp.as_ref()),
        }
    }
}

// ── StorageClassSummary ───────────────────────────────────────────────────────

/// Summary of a cluster-scoped StorageClass.
#[derive(Debug, Clone)]
pub struct StorageClassSummary {
    pub name: String,
    pub provisioner: String,
    pub reclaim_policy: String,
    pub age: String,
}

impl From<K8sStorageClass> for StorageClassSummary {
    fn from(sc: K8sStorageClass) -> Self {
        StorageClassSummary {
            name: sc.metadata.name.clone().unwrap_or_default(),
            provisioner: sc.provisioner.clone(),
            reclaim_policy: sc.reclaim_policy.clone().unwrap_or_else(|| "Delete".to_string()),
            age: age_from_ts(sc.metadata.creation_timestamp.as_ref()),
        }
    }
}

// ── ResourceTree ──────────────────────────────────────────────────────────────

/// The kind of a node in the hierarchical resource tree.
#[derive(Debug, Clone, PartialEq)]
pub enum TreeNodeKind {
    ClusterRoot,
    Namespace,
    /// A synthetic folder grouping resources of one kind (e.g. "Deployments").
    KindGroup(String),
    Deployment,
    ReplicaSet,
    StatefulSet,
    DaemonSet,
    Job,
    Pod,
    /// Container embedded inside a Pod (non-selectable leaf).
    Container,
    Service,
    Ingress,
    ConfigMap,
    Node,
    HorizontalPodAutoscaler,
    StorageClass,
}

/// A single node in the hierarchical Kubernetes resource tree.
#[derive(Debug, Clone)]
pub struct ResourceTreeNode {
    /// Unique stable ID, e.g. `"deploy:default/nginx"`.
    pub id: String,
    pub kind: TreeNodeKind,
    pub name: String,
    pub namespace: Option<String>,
    pub status: Option<String>,
    pub scope: ResourceScope,
    pub labels: BTreeMap<String, String>,
    pub relationships: Vec<ResourceRelationship>,
    pub children: Vec<ResourceTreeNode>,
}

impl ResourceTreeNode {
    fn leaf(id: String, kind: TreeNodeKind, name: String, namespace: Option<String>, status: Option<String>, scope: ResourceScope) -> Self {
        Self { id, kind, name, namespace, status, scope, labels: BTreeMap::new(), relationships: vec![], children: vec![] }
    }

    fn group(id: String, label: &str, namespace: Option<String>, children: Vec<ResourceTreeNode>) -> Self {
        Self {
            id, kind: TreeNodeKind::KindGroup(label.to_string()), name: label.to_string(),
            namespace, status: None, scope: ResourceScope::Cluster,
            labels: BTreeMap::new(), relationships: vec![], children,
        }
    }
}

/// Check whether every key-value pair in `selector` is present in `labels`.
fn selector_matches(selector: &BTreeMap<String, String>, labels: &BTreeMap<String, String>) -> bool {
    selector.iter().all(|(k, v)| labels.get(k).map(|lv| lv == v).unwrap_or(false))
}

/// Build a hierarchical resource tree from flat watcher snapshots.
///
/// Structure: ClusterRoot → [Cluster Resources group] [Nodes group] [Namespace…]
/// Namespace → [Deployments group (→ RS → Pod → Container)] [Services] [Ingresses] [ConfigMaps] [Standalone Pods]
#[allow(clippy::too_many_arguments)]
pub fn build_resource_tree(
    cluster_name: &str,
    namespaces: &[String],
    pods: &[PodSummary],
    deployments: &[DeploymentSummary],
    replica_sets: &[ReplicaSetSummary],
    services: &[ServiceSummary],
    configmaps: &[ConfigMapSummary],
    nodes: &[NodeSummary],
    ingresses: &[IngressSummary],
    hpas: &[HpaSummary],
    storage_classes: &[StorageClassSummary],
) -> ResourceTreeNode {
    let mut cluster_children: Vec<ResourceTreeNode> = Vec::new();

    // Cluster-scoped resources (StorageClasses)
    if !storage_classes.is_empty() {
        let sc_nodes: Vec<ResourceTreeNode> = storage_classes
            .iter()
            .map(|sc| ResourceTreeNode::leaf(
                format!("sc:{}", sc.name),
                TreeNodeKind::StorageClass,
                sc.name.clone(),
                None,
                Some(sc.provisioner.clone()),
                ResourceScope::Cluster,
            ))
            .collect();
        cluster_children.push(ResourceTreeNode::group("group:cluster-resources".to_string(), "Cluster Resources", None, sc_nodes));
    }

    // Cluster-scoped Nodes
    if !nodes.is_empty() {
        let node_children: Vec<ResourceTreeNode> = nodes
            .iter()
            .map(|n| ResourceTreeNode::leaf(
                format!("node:{}", n.name),
                TreeNodeKind::Node,
                n.name.clone(),
                None,
                Some(n.status.clone()),
                ResourceScope::Cluster,
            ))
            .collect();
        cluster_children.push(ResourceTreeNode::group("group:nodes".to_string(), "Nodes", None, node_children));
    }

    // Per-namespace resources
    for ns in namespaces {
        let ns_pods: Vec<&PodSummary> = pods.iter().filter(|p| &p.namespace == ns).collect();
        let ns_deploys: Vec<&DeploymentSummary> = deployments.iter().filter(|d| &d.namespace == ns).collect();
        let ns_rsets: Vec<&ReplicaSetSummary> = replica_sets.iter().filter(|r| &r.namespace == ns).collect();
        let ns_services: Vec<&ServiceSummary> = services.iter().filter(|s| &s.namespace == ns).collect();
        let ns_configmaps: Vec<&ConfigMapSummary> = configmaps.iter().filter(|c| &c.namespace == ns).collect();
        let ns_ingresses: Vec<&IngressSummary> = ingresses.iter().filter(|i| &i.namespace == ns).collect();
        let ns_hpas: Vec<&HpaSummary> = hpas.iter().filter(|h| &h.namespace == ns).collect();

        let mut ns_children: Vec<ResourceTreeNode> = Vec::new();

        // Build pod node helper (Pod → Container leaves)
        let build_pod_node = |p: &&PodSummary| -> ResourceTreeNode {
            let container_leaves: Vec<ResourceTreeNode> = p.container_names.iter().map(|cn| {
                ResourceTreeNode::leaf(
                    format!("container:{}/{}/{}", ns, p.name, cn),
                    TreeNodeKind::Container,
                    cn.clone(),
                    Some(ns.clone()),
                    None,
                    ResourceScope::Embedded,
                )
            }).collect();
            ResourceTreeNode {
                id: format!("pod:{}/{}", ns, p.name),
                kind: TreeNodeKind::Pod,
                name: p.name.clone(),
                namespace: Some(ns.clone()),
                status: Some(p.status.clone()),
                scope: ResourceScope::Namespaced,
                labels: p.labels.clone(),
                relationships: p.references.clone(),
                children: container_leaves,
            }
        };

        // Deployments → ReplicaSets → Pods → Containers
        if !ns_deploys.is_empty() {
            let deploy_nodes: Vec<ResourceTreeNode> = ns_deploys
                .iter()
                .map(|d| {
                    // Find HPAs targeting this deployment
                    let hpa_rels: Vec<ResourceRelationship> = ns_hpas.iter()
                        .filter(|h| h.target_kind == "Deployment" && h.target_name == d.name)
                        .map(|h| ResourceRelationship {
                            rel_type: RelationType::TargetedBy,
                            kind: "HorizontalPodAutoscaler".to_string(),
                            name: h.name.clone(),
                            namespace: Some(ns.clone()),
                        })
                        .collect();

                    // Find ReplicaSets owned by this Deployment
                    let rs_nodes: Vec<ResourceTreeNode> = ns_rsets.iter()
                        .filter(|r| r.owner_deployment.as_deref() == Some(d.name.as_str()))
                        .map(|r| {
                            // Pods owned by this RS
                            let rs_pods: Vec<ResourceTreeNode> = ns_pods.iter()
                                .filter(|p| p.owner_kind == "ReplicaSet" && p.owner_name == r.name)
                                .map(build_pod_node)
                                .collect();
                            ResourceTreeNode {
                                id: format!("rs:{}/{}", ns, r.name),
                                kind: TreeNodeKind::ReplicaSet,
                                name: r.name.clone(),
                                namespace: Some(ns.clone()),
                                status: Some(format!("{}/{}", r.ready, r.desired)),
                                scope: ResourceScope::Namespaced,
                                labels: BTreeMap::new(),
                                relationships: vec![],
                                children: rs_pods,
                            }
                        })
                        .collect();

                    // Fallback: if no RS data, attach pods directly using prefix heuristic
                    let children = if rs_nodes.is_empty() {
                        let prefix = format!("{}-", d.name);
                        ns_pods.iter()
                            .filter(|p| p.owner_kind == "ReplicaSet" && p.owner_name.starts_with(&prefix))
                            .map(build_pod_node)
                            .collect()
                    } else {
                        rs_nodes
                    };

                    ResourceTreeNode {
                        id: format!("deploy:{}/{}", ns, d.name),
                        kind: TreeNodeKind::Deployment,
                        name: d.name.clone(),
                        namespace: Some(ns.clone()),
                        status: Some(d.ready.clone()),
                        scope: ResourceScope::Namespaced,
                        labels: BTreeMap::new(),
                        relationships: hpa_rels,
                        children,
                    }
                })
                .collect();
            ns_children.push(ResourceTreeNode::group(
                format!("group:{}/deployments", ns),
                "Deployments",
                Some(ns.clone()),
                deploy_nodes,
            ));
        }

        // Services (with Selects relationships computed from label selector)
        if !ns_services.is_empty() {
            let svc_nodes: Vec<ResourceTreeNode> = ns_services
                .iter()
                .map(|s| {
                    let selects_rels: Vec<ResourceRelationship> = if !s.selector.is_empty() {
                        ns_pods.iter()
                            .filter(|p| selector_matches(&s.selector, &p.labels))
                            .map(|p| ResourceRelationship {
                                rel_type: RelationType::Selects,
                                kind: "Pod".to_string(),
                                name: p.name.clone(),
                                namespace: Some(ns.clone()),
                            })
                            .take(3) // cap at 3 to avoid overwhelming the badge list
                            .collect()
                    } else { vec![] };
                    ResourceTreeNode {
                        id: format!("svc:{}/{}", ns, s.name),
                        kind: TreeNodeKind::Service,
                        name: s.name.clone(),
                        namespace: Some(ns.clone()),
                        status: Some(s.type_.clone()),
                        scope: ResourceScope::Namespaced,
                        labels: BTreeMap::new(),
                        relationships: selects_rels,
                        children: vec![],
                    }
                })
                .collect();
            ns_children.push(ResourceTreeNode::group(
                format!("group:{}/services", ns),
                "Services",
                Some(ns.clone()),
                svc_nodes,
            ));
        }

        // Ingresses (with RoutesTo relationships)
        if !ns_ingresses.is_empty() {
            let ing_nodes: Vec<ResourceTreeNode> = ns_ingresses
                .iter()
                .map(|i| {
                    let routes_rels: Vec<ResourceRelationship> = i.backend_services.iter()
                        .map(|svc| ResourceRelationship {
                            rel_type: RelationType::RoutesTo,
                            kind: "Service".to_string(),
                            name: svc.clone(),
                            namespace: Some(ns.clone()),
                        })
                        .collect();
                    ResourceTreeNode {
                        id: format!("ing:{}/{}", ns, i.name),
                        kind: TreeNodeKind::Ingress,
                        name: i.name.clone(),
                        namespace: Some(ns.clone()),
                        status: None,
                        scope: ResourceScope::Namespaced,
                        labels: BTreeMap::new(),
                        relationships: routes_rels,
                        children: vec![],
                    }
                })
                .collect();
            ns_children.push(ResourceTreeNode::group(
                format!("group:{}/ingresses", ns),
                "Ingresses",
                Some(ns.clone()),
                ing_nodes,
            ));
        }

        // ConfigMaps
        if !ns_configmaps.is_empty() {
            let cm_nodes: Vec<ResourceTreeNode> = ns_configmaps
                .iter()
                .map(|c| ResourceTreeNode::leaf(
                    format!("cm:{}/{}", ns, c.name),
                    TreeNodeKind::ConfigMap,
                    c.name.clone(),
                    Some(ns.clone()),
                    None,
                    ResourceScope::Namespaced,
                ))
                .collect();
            ns_children.push(ResourceTreeNode::group(
                format!("group:{}/configmaps", ns),
                "ConfigMaps",
                Some(ns.clone()),
                cm_nodes,
            ));
        }

        // Standalone pods — not owned by any ReplicaSet that belongs to a known Deployment
        let known_rs_names: std::collections::HashSet<&str> = ns_rsets.iter()
            .filter(|r| r.owner_deployment.is_some())
            .map(|r| r.name.as_str())
            .collect();
        let remaining: Vec<ResourceTreeNode> = ns_pods
            .iter()
            .filter(|p| {
                if p.owner_kind == "ReplicaSet" {
                    !known_rs_names.contains(p.owner_name.as_str()) &&
                    !ns_deploys.iter().any(|d| p.owner_name.starts_with(&format!("{}-", d.name)))
                } else {
                    true
                }
            })
            .map(build_pod_node)
            .collect();

        if !remaining.is_empty() {
            ns_children.push(ResourceTreeNode::group(
                format!("group:{}/pods", ns),
                "Pods",
                Some(ns.clone()),
                remaining,
            ));
        }

        if !ns_children.is_empty() {
            cluster_children.push(ResourceTreeNode {
                id: format!("ns:{}", ns),
                kind: TreeNodeKind::Namespace,
                name: ns.clone(),
                namespace: None,
                status: None,
                scope: ResourceScope::Cluster,
                labels: BTreeMap::new(),
                relationships: vec![],
                children: ns_children,
            });
        }
    }

    ResourceTreeNode {
        id: format!("cluster:{}", cluster_name),
        kind: TreeNodeKind::ClusterRoot,
        name: cluster_name.to_string(),
        namespace: None,
        status: None,
        scope: ResourceScope::Cluster,
        labels: BTreeMap::new(),
        relationships: vec![],
        children: cluster_children,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_RUNNING_POD: &str = r#"{
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "nginx-abc",
            "namespace": "default",
            "creationTimestamp": "2026-03-27T00:00:00Z",
            "labels": {"app": "nginx"}
        },
        "spec": {
            "nodeName": "node-1",
            "containers": [
                {"name": "nginx", "image": "nginx:latest"},
                {"name": "sidecar", "image": "busybox:latest"}
            ]
        },
        "status": {
            "phase": "Running",
            "containerStatuses": [
                {
                    "name": "nginx",
                    "image": "nginx:latest",
                    "imageID": "",
                    "ready": true,
                    "restartCount": 0,
                    "state": {"running": {"startedAt": "2026-03-27T00:01:00Z"}}
                },
                {
                    "name": "sidecar",
                    "image": "busybox:latest",
                    "imageID": "",
                    "ready": true,
                    "restartCount": 0,
                    "state": {"running": {"startedAt": "2026-03-27T00:01:00Z"}}
                }
            ]
        }
    }"#;

    const FIXTURE_CRASHLOOP_POD: &str = r#"{
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "broken-xyz",
            "namespace": "default",
            "creationTimestamp": "2026-03-27T00:00:00Z"
        },
        "spec": {
            "nodeName": "node-2",
            "containers": [{"name": "app", "image": "broken:latest"}]
        },
        "status": {
            "phase": "Running",
            "containerStatuses": [
                {
                    "name": "app",
                    "image": "broken:latest",
                    "imageID": "",
                    "ready": false,
                    "restartCount": 5,
                    "state": {
                        "waiting": {
                            "reason": "CrashLoopBackOff",
                            "message": "back-off 5m0s restarting failed container"
                        }
                    }
                }
            ]
        }
    }"#;

    #[test]
    fn running_pod_summary() {
        let pod: Pod = serde_json::from_str(FIXTURE_RUNNING_POD).unwrap();
        let s = PodSummary::from(pod);
        assert_eq!(s.name, "nginx-abc");
        assert_eq!(s.namespace, "default");
        assert_eq!(s.status, "Running");
        assert_eq!(s.ready, "2/2");
        assert_eq!(s.restarts, 0);
        assert_eq!(s.node, "node-1");
    }

    #[test]
    fn crashloop_pod_summary() {
        let pod: Pod = serde_json::from_str(FIXTURE_CRASHLOOP_POD).unwrap();
        let s = PodSummary::from(pod);
        assert_eq!(s.status, "CrashLoopBackOff");
        assert_eq!(s.ready, "0/1");
        assert_eq!(s.restarts, 5);
    }

    #[test]
    fn pod_detail_events_empty_and_labels() {
        let pod: Pod = serde_json::from_str(FIXTURE_RUNNING_POD).unwrap();
        let d = PodDetail::from(pod);
        assert!(d.events.is_empty());
        assert_eq!(d.labels.get("app").map(String::as_str), Some("nginx"));
        assert_eq!(d.containers.len(), 2);
    }

    #[test]
    fn generic_detail_from_replicaset_manifest() {
        let manifest: serde_json::Value = serde_json::from_str(r#"{
            "metadata": {"creationTimestamp": "2020-01-01T00:00:00Z"},
            "spec": {"replicas": 3},
            "status": {"readyReplicas": 2}
        }"#).unwrap();
        let d = GenericResourceDetail::from_manifest("ReplicaSet", "default", "web-abc", &manifest);
        assert_eq!(d.kind, "ReplicaSet");
        assert_eq!(d.status_summary.as_deref(), Some("2/3 ready"));
        assert!(d.age.is_some());
        assert!(d.error.is_none());
        assert!(!d.loading);
    }

    #[test]
    fn generic_detail_from_job_complete() {
        let manifest: serde_json::Value = serde_json::from_str(r#"{
            "metadata": {},
            "status": {"succeeded": 1}
        }"#).unwrap();
        let d = GenericResourceDetail::from_manifest("Job", "default", "backup", &manifest);
        assert_eq!(d.status_summary.as_deref(), Some("Complete"));
    }

    #[test]
    fn generic_detail_missing_status_falls_back() {
        let manifest: serde_json::Value = serde_json::from_str(r#"{
            "metadata": {"creationTimestamp": "2020-01-01T00:00:00Z"}
        }"#).unwrap();
        let d = GenericResourceDetail::from_manifest("Ingress", "default", "x", &manifest);
        assert!(d.status_summary.is_none());
    }

    #[test]
    fn generic_detail_unavailable() {
        let d = GenericResourceDetail::unavailable("Pod", "default", "ghost", "404".into());
        assert_eq!(d.error.as_deref(), Some("404"));
        assert!(!d.loading);
    }

    #[test]
    fn generic_detail_loading() {
        let d = GenericResourceDetail::loading("Job", "default", "backup");
        assert!(d.loading);
        assert!(d.error.is_none());
        assert!(d.status_summary.is_none());
    }
}
