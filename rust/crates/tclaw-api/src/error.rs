use serde::de::DeserializeOwned;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::http_client::{HttpClientError, HttpResponse};
use crate::types::ProviderKind;

#[derive(Debug)]
pub enum ApiError {
    Http(HttpClientError),
    Status {
        status: u16,
        body: String,
    },
    Serialize(String),
    InvalidResponse {
        message: String,
    },
    Decode {
        source: serde_json::Error,
        body: String,
    },
    SseParse(String),
    Provider {
        provider: ProviderKind,
        message: String,
    },
    UnsupportedStream {
        provider: ProviderKind,
    },
}

impl ApiError {
    pub fn decode_json<T: DeserializeOwned>(response: &HttpResponse) -> Result<T, Self> {
        serde_json::from_slice::<T>(&response.body).map_err(|source| Self::Decode {
            source,
            body: String::from_utf8_lossy(&response.body).into_owned(),
        })
    }
}

impl Display for ApiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(err) => write!(f, "{err}"),
            Self::Status { status, .. } => write!(f, "unexpected http status {status}"),
            Self::Serialize(message) => write!(f, "serialization error: {message}"),
            Self::InvalidResponse { message } => write!(f, "invalid response body: {message}"),
            Self::Decode { source, .. } => write!(f, "failed to decode json response: {source}"),
            Self::SseParse(message) => write!(f, "invalid sse payload: {message}"),
            Self::Provider { provider, message } => {
                write!(f, "{provider:?} provider error: {message}")
            }
            Self::UnsupportedStream { provider } => {
                write!(f, "{provider:?} provider does not support streaming")
            }
        }
    }
}

impl Error for ApiError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Http(err) => Some(err),
            Self::Decode { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<HttpClientError> for ApiError {
    fn from(value: HttpClientError) -> Self {
        Self::Http(value)
    }
}
