use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonEnvelope {
    pub kind: String,
    pub payload: Value,
}

impl JsonEnvelope {
    pub fn parse(input: &str) -> Result<Self, JsonEnvelopeError> {
        serde_json::from_str(input).map_err(JsonEnvelopeError::from)
    }
}

#[derive(Debug, Error)]
pub enum JsonEnvelopeError {
    #[error("invalid json payload: {0}")]
    InvalidPayload(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_envelope_parses_boundary_payloads() {
        let envelope = JsonEnvelope::parse(r#"{"kind":"prompt","payload":{"id":1}}"#)
            .expect("parse envelope");

        assert_eq!(envelope.kind, "prompt");
        assert_eq!(envelope.payload["id"], 1);
    }
}
