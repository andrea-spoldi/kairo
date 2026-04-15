/// Emitted by event cards when the user clicks "Analyze" to request AI analysis.
/// The payload is a JSON string encoding the event context.
#[derive(Clone, Debug)]
pub struct AnalyzeEventRequest(pub String);
