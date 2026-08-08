use serde::Serialize;

/// Generic JSON error envelope returned by every non-success response.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    /// Stable machine-readable error code (e.g. `"unauthorized"`, `"not_found"`).
    pub error: String,
}

impl ErrorBody {
    /// Creates a new error body carrying the given machine-readable code.
    pub fn new(code: impl Into<String>) -> Self {
        Self { error: code.into() }
    }
}
