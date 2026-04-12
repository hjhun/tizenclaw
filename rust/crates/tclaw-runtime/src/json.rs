use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use std::fmt::{Display, Formatter};

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

#[derive(Debug)]
pub enum JsonEnvelopeError {
    InvalidPayload(serde_json::Error),
}

impl Display for JsonEnvelopeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPayload(err) => write!(f, "invalid json payload: {err}"),
        }
    }
}

impl Error for JsonEnvelopeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidPayload(err) => Some(err),
        }
    }
}

impl From<serde_json::Error> for JsonEnvelopeError {
    fn from(value: serde_json::Error) -> Self {
        Self::InvalidPayload(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_envelope_parses_boundary_payloads() {
        let envelope =
            JsonEnvelope::parse(r#"{"kind":"prompt","payload":{"id":1}}"#).expect("parse envelope");

        assert_eq!(envelope.kind, "prompt");
        assert_eq!(envelope.payload["id"], 1);
    }
}
