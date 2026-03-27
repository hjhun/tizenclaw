//! HTTP client using ureq with Tizen CA certificate support.
//!
//! Provides GET/POST with retry, exponential backoff, and automatic
//! CA certificate discovery matching the C++ tizenclaw_curl behavior.

use std::sync::OnceLock;

/// Tizen-compatible CA certificate paths probed at startup.
const CA_CERT_PATHS: &[&str] = &[
    "/etc/ssl/certs/ca-certificates.crt",
    "/etc/ssl/ca-bundle.pem",
    "/etc/pki/tls/certs/ca-bundle.crt",
    "/usr/share/ca-certificates/ca-bundle.crt",
];

/// Cached ureq agent with proper TLS configuration.
static AGENT: OnceLock<ureq::Agent> = OnceLock::new();

pub struct HttpResponse {
    pub status_code: u16,
    pub body: String,
    pub success: bool,
    pub error: String,
}

/// Build a properly configured ureq agent with CA certificates.
fn build_agent(timeout_secs: u64) -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(timeout_secs));

    // Probe for CA certificate file (matching C++ tizenclaw_curl behavior)
    for path in CA_CERT_PATHS {
        if std::path::Path::new(path).exists() {
            log::info!("Using CA cert bundle: {}", path);
            match std::fs::read(path) {
                Ok(pem_data) => {
                    // Parse all certificates from the PEM bundle
                    let certs = parse_pem_certs(&pem_data);
                    if !certs.is_empty() {
                        let mut root_store = Vec::new();
                        for cert_der in &certs {
                            if let Ok(cert) = native_tls::Certificate::from_der(cert_der) {
                                root_store.push(cert);
                            } else if let Ok(cert) = native_tls::Certificate::from_pem(cert_der) {
                                root_store.push(cert);
                            }
                        }
                        // Build TLS connector with custom CA certs
                        let mut tls_builder = native_tls::TlsConnector::builder();
                        for cert in root_store {
                            tls_builder.add_root_certificate(cert);
                        }
                        if let Ok(connector) = tls_builder.build() {
                            builder = builder.tls_connector(std::sync::Arc::new(connector));
                            log::info!("TLS configured with {} CA certs from {}", certs.len(), path);
                        }
                    }
                }
                Err(e) => log::warn!("Failed to read CA bundle {}: {}", path, e),
            }
            break;
        }
    }

    builder.build()
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
pub fn http_post(
    url: &str,
    headers: &[(&str, &str)],
    json_body: &str,
    max_retries: u32,
    timeout_secs: u64,
) -> HttpResponse {
    for attempt in 0..=max_retries {
        if attempt > 0 {
            let delay = std::time::Duration::from_millis(500 * (1 << (attempt - 1)));
            log::info!("HTTP retry {} after {}ms", attempt, delay.as_millis());
            std::thread::sleep(delay);
        }
        match do_post(url, headers, json_body, timeout_secs) {
            Ok(resp) if resp.status_code == 429 || resp.status_code >= 500 => {
                log::warn!("HTTP {}, retrying ({}/{})", resp.status_code, attempt + 1, max_retries);
                if attempt == max_retries {
                    return resp;
                }
            }
            Ok(resp) => return resp,
            Err(e) => {
                log::warn!("HTTP POST failed: {} ({}/{})", e, attempt + 1, max_retries);
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
pub fn http_get(
    url: &str,
    headers: &[(&str, &str)],
    max_retries: u32,
    timeout_secs: u64,
) -> HttpResponse {
    for attempt in 0..=max_retries {
        if attempt > 0 {
            let delay = std::time::Duration::from_millis(500 * (1 << (attempt - 1)));
            std::thread::sleep(delay);
        }
        match do_get(url, headers, timeout_secs) {
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

fn do_post(url: &str, headers: &[(&str, &str)], body: &str, timeout_secs: u64) -> Result<HttpResponse, String> {
    let agent = build_agent(timeout_secs);

    let mut req = agent.post(url);
    for (k, v) in headers {
        req = req.set(k, v);
    }
    req = req.set("Content-Type", "application/json");

    match req.send_string(body) {
        Ok(resp) => {
            let status = resp.status();
            let body_str = resp.into_string().unwrap_or_default();
            Ok(HttpResponse {
                status_code: status,
                body: body_str,
                success: (200..300).contains(&status),
                error: String::new(),
            })
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body_str = resp.into_string().unwrap_or_default();
            Ok(HttpResponse {
                status_code: code,
                body: body_str,
                success: false,
                error: format!("HTTP {}", code),
            })
        }
        Err(e) => Err(format!("Connection Failed: {}", e)),
    }
}

fn do_get(url: &str, headers: &[(&str, &str)], timeout_secs: u64) -> Result<HttpResponse, String> {
    let agent = build_agent(timeout_secs);

    let mut req = agent.get(url);
    for (k, v) in headers {
        req = req.set(k, v);
    }

    match req.call() {
        Ok(resp) => {
            let status = resp.status();
            let body_str = resp.into_string().unwrap_or_default();
            Ok(HttpResponse {
                status_code: status,
                body: body_str,
                success: (200..300).contains(&status),
                error: String::new(),
            })
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body_str = resp.into_string().unwrap_or_default();
            Ok(HttpResponse {
                status_code: code,
                body: body_str,
                success: false,
                error: format!("HTTP {}", code),
            })
        }
        Err(e) => Err(format!("Connection Failed: {}", e)),
    }
}

/// Convenience struct for channels/MCP.
pub struct HttpClient;

impl HttpClient {
    pub fn new() -> Self { HttpClient }

    pub fn get(&self, url: &str) -> Result<HttpResponse, Box<dyn std::error::Error>> {
        let r = http_get(url, &[], 1, 30);
        if r.success { Ok(r) } else { Err(r.error.into()) }
    }

    pub fn post(&self, url: &str, body: &str) -> Result<HttpResponse, Box<dyn std::error::Error>> {
        let r = http_post(url, &[], body, 1, 30);
        if r.success { Ok(r) } else { Err(r.error.into()) }
    }
}
