#[derive(Debug, Clone, Copy)]
pub struct StreamPlan {
    /// If true, suppress raw stdout/stderr forwarding in the tee.
    /// This is typically used when the process output is expected to be clean JSONL.
    pub silent: bool,
}
