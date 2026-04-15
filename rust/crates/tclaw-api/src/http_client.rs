use std::collections::{BTreeMap, VecDeque};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Mutex;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    pub fn json(
        method: HttpMethod,
        url: impl Into<String>,
        body: Value,
    ) -> Result<Self, HttpClientError> {
        let mut headers = BTreeMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        Ok(Self {
            method,
            url: url.into(),
            headers,
            body: serde_json::to_vec(&body).map_err(|source| HttpClientError::Serialize {
                message: source.to_string(),
            })?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn json(status: u16, body: Value) -> Result<Self, HttpClientError> {
        let mut headers = BTreeMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        Ok(Self {
            status,
            headers,
            body: serde_json::to_vec(&body).map_err(|source| HttpClientError::Serialize {
                message: source.to_string(),
            })?,
        })
    }

    pub fn text(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            headers: BTreeMap::new(),
            body: body.into().into_bytes(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpClientError {
    Transport { message: String },
    Serialize { message: String },
    Exhausted,
}

impl Display for HttpClientError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport { message } => write!(f, "transport failure: {message}"),
            Self::Serialize { message } => write!(f, "failed to serialize request: {message}"),
            Self::Exhausted => write!(f, "static response queue exhausted"),
        }
    }
}

impl Error for HttpClientError {}

pub trait HttpClient: Send + Sync {
    fn execute(&self, request: HttpRequest) -> Result<HttpResponse, HttpClientError>;
}

#[derive(Debug, Default)]
pub struct StaticHttpClient {
    requests: Mutex<Vec<HttpRequest>>,
    responses: Mutex<VecDeque<Result<HttpResponse, HttpClientError>>>,
}

impl StaticHttpClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_response(&self, response: Result<HttpResponse, HttpClientError>) {
        self.responses
            .lock()
            .expect("static http responses mutex poisoned")
            .push_back(response);
    }

    pub fn push_json_response(&self, status: u16, body: Value) {
        self.push_response(HttpResponse::json(status, body));
    }

    pub fn push_text_response(&self, status: u16, body: impl Into<String>) {
        self.push_response(Ok(HttpResponse::text(status, body)));
    }

    pub fn take_requests(&self) -> Vec<HttpRequest> {
        std::mem::take(
            &mut *self
                .requests
                .lock()
                .expect("static http requests mutex poisoned"),
        )
    }
}

impl HttpClient for StaticHttpClient {
    fn execute(&self, request: HttpRequest) -> Result<HttpResponse, HttpClientError> {
        self.requests
            .lock()
            .expect("static http requests mutex poisoned")
            .push(request);
        self.responses
            .lock()
            .expect("static http responses mutex poisoned")
            .pop_front()
            .unwrap_or(Err(HttpClientError::Exhausted))
    }
}
