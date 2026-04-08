use std::collections::BTreeMap;

use chrono::Utc;
use k8s_openapi::api::core::v1::{
    ContainerState, ContainerStatus as K8sContainerStatus, Event as K8sEvent, Pod,
};

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

        PodSummary { name, namespace, status, ready, restarts, age, node, labels, owner_kind, owner_name }
    }
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
    let created_unix = match pod.metadata.creation_timestamp.as_ref() {
        Some(t) => t.0.as_second(),
        None => return "?".to_string(),
    };
    let now_unix = Utc::now().timestamp();
    let secs = (now_unix - created_unix).max(0);

    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct ClusterEvent {
    /// Namespace where the event originated.
    pub namespace: String,
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
        let object_name = ev.involved_object.name.clone().unwrap_or_default();
        let namespace = ev.metadata.namespace.clone().unwrap_or_default();
        ClusterEvent {
            namespace,
            object_name,
            reason: ev.reason.unwrap_or_default(),
            message: ev.message.unwrap_or_default(),
            event_type: ev.type_.unwrap_or_default(),
            count: ev.count.unwrap_or(0),
            last_time,
        }
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
}
