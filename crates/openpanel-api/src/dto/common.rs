use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}

impl ErrorBody {
    pub fn new(code: impl Into<String>) -> Self {
        Self { error: code.into() }
    }
}
