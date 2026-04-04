//! HTTP client using reqwest with Tizen CA certificate support.
//!
//! Provides GET/POST with retry, exponential backoff, and automatic
//! CA certificate discovery matching the C++ tizenclaw_curl behavior.

use std::sync::OnceLock;
use reqwest::{Client, ClientBuilder, Certificate};

/// Tizen-compatible CA certificate paths probed at startup.
const CA_CERT_PATHS: &[&str] = &[
    "/etc/ssl/certs/ca-certificates.crt",
    "/etc/ssl/ca-bundle.pem",
    "/etc/pki/tls/certs/ca-bundle.crt",
    "/usr/share/ca-certificates/ca-bundle.crt",
];

/// Cached reqwest client with proper TLS configuration.
static AGENT: OnceLock<Client> = OnceLock::new();

pub struct HttpResponse {
    pub status_code: u16,
    pub body: String,
    pub success: bool,
    pub error: String,
}

/// Retrieve or initialize the shared reqwest HTTP client, configured for Tizen TLS.
pub fn default_client() -> Client {
    AGENT.get_or_init(|| build_agent(120)).clone()
}

/// Build a properly configured reqwest client with CA certificates.
fn build_agent(timeout_secs: u64) -> Client {
    let mut builder = ClientBuilder::new()
        .timeout(std::time::Duration::from_secs(timeout_secs));

    // Probe for CA certificate file (matching C++ tizenclaw_curl behavior)
    for path in CA_CERT_PATHS {
        if std::path::Path::new(path).exists() {
            log::debug!("Using CA cert bundle: {}", path);
            match std::fs::read(path) {
                Ok(pem_data) => {
                    // Parse all certificates from the PEM bundle
                    let certs = parse_pem_certs(&pem_data);
                    if !certs.is_empty() {
                        let mut count = 0;
                        for cert_der in certs {
                            if let Ok(cert) = Certificate::from_der(&cert_der) {
                                builder = builder.add_root_certificate(cert);
                                count += 1;
                            }
                        }
                        log::debug!("TLS configured with {} CA certs from {}", count, path);
                    }
                }
                Err(e) => log::warn!("Failed to read CA bundle {}: {}", path, e),
            }
            break;
        }
    }

    builder.build().unwrap_or_else(|e| {
        log::error!("Failed to build reqwest client: {}", e);
        Client::new()
    })
}

/// Parse PEM-encoded certificates, returning DER-encoded cert data.
fn parse_pem_certs(pem_data: &[u8]) -> Vec<Vec<u8>> {
    let pem_str = match std::str::from_utf8(pem_data) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut certs = Vec::new();
    let mut in_cert = false;
    let mut current_b64 = String::new();

    for line in pem_str.lines() {
        let trimmed = line.trim();
        if trimmed == "-----BEGIN CERTIFICATE-----" {
            in_cert = true;
            current_b64.clear();
        } else if trimmed == "-----END CERTIFICATE-----" {
            in_cert = false;
            // Decode base64
            if let Ok(der) = base64_decode(&current_b64) {
                certs.push(der);
            }
            current_b64.clear();
        } else if in_cert {
            current_b64.push_str(trimmed);
        }
    }

    certs
}

/// Simple base64 decoder (no external dependency).
fn base64_decode(input: &str) -> Result<Vec<u8>, ()> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn decode_char(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let bytes: Vec<u8> = input.bytes().filter(|b| *b != b'\n' && *b != b'\r' && *b != b' ').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);

    let mut i = 0;
    while i + 3 < bytes.len() {
        let a = decode_char(bytes[i]).ok_or(())?;
        let b = decode_char(bytes[i + 1]).ok_or(())?;
        out.push((a << 2) | (b >> 4));

        if bytes[i + 2] != b'=' {
            let c = decode_char(bytes[i + 2]).ok_or(())?;
            out.push((b << 4) | (c >> 2));
            if bytes[i + 3] != b'=' {
                let d = decode_char(bytes[i + 3]).ok_or(())?;
                out.push((c << 6) | d);
            }
        }
        i += 4;
    }

    Ok(out)
}

/// POST JSON to a URL with retry and backoff.
pub async fn http_post(
    url: &str,
    headers: &[(&str, &str)],
    json_body: &str,
    max_retries: u32,
    timeout_secs: u64,
) -> HttpResponse {
    for attempt in 0..=max_retries {
        if attempt > 0 {
            let delay = std::time::Duration::from_millis(500 * (1 << (attempt - 1)));
            log::debug!("HTTP retry {} after {}ms", attempt, delay.as_millis());
            tokio::time::sleep(delay).await;
        }
        match do_post(url, headers, json_body, timeout_secs).await {
            Ok(resp) if resp.status_code == 429 || resp.status_code >= 500 => {
                log::warn!("HTTP {}, retrying ({}/{})", resp.status_code, attempt + 1, max_retries);
                if attempt == max_retries {
                    return resp;
                }
            }
            Ok(resp) => return resp,
            Err(e) => {
                let err_str = format!("HTTP POST failed: {} ({}/{})\n", e, attempt + 1, max_retries);
                log::warn!("{}", err_str.trim());
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/http_err.log") {
                    use std::io::Write;
                    let _ = f.write_all(err_str.as_bytes());
                }
                
                if attempt == max_retries {
                    return HttpResponse {
                        status_code: 0,
                        body: String::new(),
                        success: false,
                        error: e,
                    };
                }
            }
        }
    }
    unreachable!()
}

/// GET a URL with retry.
pub async fn http_get(
    url: &str,
    headers: &[(&str, &str)],
    max_retries: u32,
    timeout_secs: u64,
) -> HttpResponse {
    for attempt in 0..=max_retries {
        if attempt > 0 {
            let delay = std::time::Duration::from_millis(500 * (1 << (attempt - 1)));
            tokio::time::sleep(delay).await;
        }
        match do_get(url, headers, timeout_secs).await {
            Ok(resp) => return resp,
            Err(e) => {
                if attempt == max_retries {
                    return HttpResponse {
                        status_code: 0,
                        body: String::new(),
                        success: false,
                        error: e,
                    };
                }
            }
        }
    }
    unreachable!()
}

pub fn http_get_sync(url: &str, headers: &[(&str, &str)], max_retries: u32, timeout_secs: u64) -> HttpResponse {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(http_get(url, headers, max_retries, timeout_secs)))
    } else {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(http_get(url, headers, max_retries, timeout_secs))
    }
}

pub fn http_post_sync(url: &str, headers: &[(&str, &str)], json_body: &str, max_retries: u32, timeout_secs: u64) -> HttpResponse {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(http_post(url, headers, json_body, max_retries, timeout_secs)))
    } else {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(http_post(url, headers, json_body, max_retries, timeout_secs))
    }
}

async fn do_post(url: &str, headers: &[(&str, &str)], body: &str, _timeout_secs: u64) -> Result<HttpResponse, String> {
    let agent = default_client();

    let mut req = agent.post(url);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    req = req.header("Content-Type", "application/json");

    match req.body(body.to_string()).send().await {
        Ok(resp) => {
            let status = resp.status();
            match resp.text().await {
                Ok(body_str) => {
                    if !status.is_success() {
                        let err_str = format!("HTTP {} from {}:\n{}\n", status.as_u16(), url, body_str);
                        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/http_err.log") {
                            use std::io::Write;
                            let _ = f.write_all(err_str.as_bytes());
                        }
                    }
                    Ok(HttpResponse {
                        status_code: status.as_u16(),
                        body: body_str,
                        success: status.is_success(),
                        error: if status.is_success() { String::new() } else { format!("HTTP {}", status.as_u16()) },
                    })
                },
                Err(e) => Err(format!("Failed to read body: {}", e)),
            }
        }
        Err(e) => Err(format!("Connection Failed: {}", e)),
    }
}

async fn do_get(url: &str, headers: &[(&str, &str)], _timeout_secs: u64) -> Result<HttpResponse, String> {
    let agent = default_client();

    let mut req = agent.get(url);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            match resp.text().await {
                Ok(body_str) => Ok(HttpResponse {
                    status_code: status.as_u16(),
                    body: body_str,
                    success: status.is_success(),
                    error: if status.is_success() { String::new() } else { format!("HTTP {}", status.as_u16()) },
                }),
                Err(e) => Err(format!("Failed to read body: {}", e)),
            }
        }
        Err(e) => Err(format!("Connection Failed: {}", e)),
    }
}

/// Convenience struct for channels/MCP.
pub struct HttpClient;

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    pub fn new() -> Self { HttpClient }

    pub async fn get(&self, url: &str) -> Result<HttpResponse, Box<dyn std::error::Error + Send + Sync>> {
        let r = http_get(url, &[], 1, 30).await;
        if r.success { Ok(r) } else { Err(r.error.into()) }
    }

    pub async fn post(&self, url: &str, body: &str) -> Result<HttpResponse, Box<dyn std::error::Error + Send + Sync>> {
        let r = http_post(url, &[], body, 1, 30).await;
        if r.success { Ok(r) } else { Err(r.error.into()) }
    }

    pub fn get_sync(&self, url: &str) -> Result<HttpResponse, Box<dyn std::error::Error + Send + Sync>> {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(self.get(url)))
        } else {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(self.get(url))
        }
    }

    pub fn post_sync(&self, url: &str, body: &str) -> Result<HttpResponse, Box<dyn std::error::Error + Send + Sync>> {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(self.post(url, body)))
        } else {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(self.post(url, body))
        }
    }
}
