/// Scope binding the AI agent to a specific Kubernetes investigation context.
///
/// Drives the scope label in the AI panel and context packet injected into prompts.
#[derive(Clone, Debug, Default)]
pub enum AgentScope {
    #[default]
    None,
    Event { resource: ResourceRef, event: EventRef },
    Resource(ResourceRef),
    Log { log: LogRef, resource: Option<ResourceRef> },
}

/// Lightweight reference to a Kubernetes resource.
#[derive(Clone, Debug)]
pub struct ResourceRef {
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
}

impl ResourceRef {
    pub fn display(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("{} {}/{}", self.kind, ns, self.name),
            None => format!("{} {}", self.kind, self.name),
        }
    }
}

/// Reference to a specific Kubernetes event.
#[derive(Clone, Debug)]
pub struct EventRef {
    pub reason: String,
    pub message: String,
    pub event_type: String,
    pub count: i32,
}

/// Log context selected by the user for AI analysis.
#[derive(Clone, Debug)]
pub struct LogRef {
    /// User-selected lines, capped at 100.
    pub selected: Vec<String>,
    /// ±25 lines around the selection for context.
    pub window: Vec<String>,
    pub pod: Option<String>,
    pub container: Option<String>,
    #[allow(dead_code)]
    pub namespace: Option<String>,
}

impl AgentScope {
    /// Short human-readable label for the scope chip in the AI panel header.
    pub fn label(&self) -> Option<String> {
        match self {
            AgentScope::None => None,
            AgentScope::Event { resource, event } => {
                Some(format!("Event: {} on {}", event.reason, resource.display()))
            }
            AgentScope::Resource(r) => Some(format!("Resource: {}", r.display())),
            AgentScope::Log { log, resource } => {
                let lines = log.selected.len();
                match resource {
                    Some(r) => Some(format!("Log: {lines} lines from {}", r.display())),
                    None => Some(format!("Log: {lines} selected lines")),
                }
            }
        }
    }

    /// Quick-action suggestions shown as chips below the scope label.
    pub fn quick_actions(&self) -> &'static [&'static str] {
        match self {
            AgentScope::None => &[],
            AgentScope::Event { .. } => &["Why did this happen?", "What should I check next?", "How do I fix this?"],
            AgentScope::Resource(_) => &["Explain current status", "Check for issues", "Show recent events"],
            AgentScope::Log { .. } => &["Diagnose this error", "What caused this?", "How do I fix this?"],
        }
    }

    /// Build a Markdown context block to prepend to the AI prompt.
    pub fn context_block(&self) -> String {
        match self {
            AgentScope::None => String::new(),
            AgentScope::Event { resource, event } => {
                format!(
                    "## Investigation Scope: Event\n\
                     **Resource:** {}\n\
                     **Reason:** {}\n\
                     **Message:** {}\n\
                     **Type:** {} | **Count:** {}\n",
                    resource.display(), event.reason, event.message,
                    event.event_type, event.count
                )
            }
            AgentScope::Resource(r) => {
                format!(
                    "## Investigation Scope: Resource\n\
                     **Resource:** {}\n",
                    r.display()
                )
            }
            AgentScope::Log { log, resource } => {
                let evidence = log.selected.join("\n");
                let window = log.window.join("\n");
                let resource_line = resource
                    .as_ref()
                    .map(|r| format!("**Associated resource:** {}", r.display()))
                    .unwrap_or_default();
                format!(
                    "## Investigation Scope: Log\n\
                     {resource_line}\n\
                     **Pod:** {} | **Container:** {}\n\
                     \n\
                     ### Evidence (selected log lines)\n\
                     ```\n{evidence}\n```\n\
                     \n\
                     ### Context window (surrounding lines)\n\
                     ```\n{window}\n```\n\
                     \n\
                     When answering, first quote the relevant evidence lines verbatim, \
                     then clearly label your interpretation as inference. \
                     Do not speculate beyond what the logs support.\n",
                    log.pod.as_deref().unwrap_or("unknown"),
                    log.container.as_deref().unwrap_or("unknown"),
                )
            }
        }
    }
}
