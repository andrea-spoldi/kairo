/// Summary of a Pod for list views.
#[derive(Debug, Clone)]
pub struct PodSummary {
    pub name: String,
    pub namespace: String,
    pub status: String,
    pub ready: String,
    pub restarts: i32,
    pub age: String,
    pub node: String,
}

/// Detailed Pod information for the detail panel.
#[derive(Debug, Clone)]
pub struct PodDetail {
    pub summary: PodSummary,
    pub labels: std::collections::BTreeMap<String, String>,
    pub annotations: std::collections::BTreeMap<String, String>,
    pub containers: Vec<ContainerStatus>,
    pub events: Vec<PodEvent>,
}

/// Status of a single container within a Pod.
#[derive(Debug, Clone)]
pub struct ContainerStatus {
    pub name: String,
    pub image: String,
    pub ready: bool,
    pub restart_count: i32,
    pub state: String,
}

/// A Kubernetes event related to a Pod.
#[derive(Debug, Clone)]
pub struct PodEvent {
    pub reason: String,
    pub message: String,
    pub event_type: String,
    pub count: i32,
    pub first_time: String,
    pub last_time: String,
}
