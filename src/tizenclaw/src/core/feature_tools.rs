use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

const PDF_EXTRACTOR_SCRIPT: &str = r#"#!/usr/bin/env python3
import json
import re
import sys
import zlib
from pathlib import Path

def _extract_with_pypdf(path: Path) -> str:
    try:
        from pypdf import PdfReader
    except Exception:
        return ""

    try:
        reader = PdfReader(str(path))
        chunks = []
        for page in reader.pages:
            text = page.extract_text() or ""
            if text.strip():
                chunks.append(text)
        return "\n\n".join(chunks)
    except Exception:
        return ""

def _extract_flate_streams(raw: bytes) -> str:
    texts = []
    for match in re.finditer(rb"stream\r?\n(.*?)\r?\nendstream", raw, re.S):
        stream = match.group(1)
        for candidate in (stream,):
            try:
                decoded = zlib.decompress(candidate)
            except Exception:
                continue
            for token in re.findall(rb"\((.*?)\)\s*Tj", decoded, re.S):
                try:
                    texts.append(token.decode("latin1", errors="ignore"))
                except Exception:
                    pass
            for arr in re.findall(rb"\[(.*?)\]\s*TJ", decoded, re.S):
                parts = re.findall(rb"\((.*?)\)", arr, re.S)
                joined = "".join(part.decode("latin1", errors="ignore") for part in parts)
                if joined:
                    texts.append(joined)
    return "\n".join(texts)

def main() -> None:
    path = Path(sys.argv[1])
    raw = path.read_bytes()
    text = _extract_with_pypdf(path)
    if not text.strip():
        text = _extract_flate_streams(raw)
    text = text.replace("\\r", "\n")
    text = re.sub(r"[ \t]+", " ", text)
    text = re.sub(r"\n{3,}", "\n\n", text).strip()
    print(json.dumps({
        "status": "success",
        "text": text,
        "char_count": len(text),
        "extractor": "pypdf_or_flate_fallback"
    }, ensure_ascii=False))

if __name__ == "__main__":
    main()
"#;

const TABULAR_INSPECTOR_SCRIPT: &str = r#"#!/usr/bin/env python3
import csv
import json
import sys
import zipfile
import xml.etree.ElementTree as ET
from pathlib import Path

NS = {
    "main": "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
    "rel": "http://schemas.openxmlformats.org/package/2006/relationships",
}

def inspect_csv(path: Path, preview_rows: int) -> dict:
    with path.open("r", encoding="utf-8", newline="") as fh:
        reader = csv.reader(fh)
        rows = list(reader)
    headers = rows[0] if rows else []
    preview = rows[1:1 + preview_rows] if len(rows) > 1 else []
    return {
        "kind": "csv",
        "sheets": [{
            "name": path.name,
            "headers": headers,
            "row_count": max(len(rows) - 1, 0),
            "preview_rows": preview,
        }]
    }

def shared_strings(zf: zipfile.ZipFile) -> list[str]:
    if "xl/sharedStrings.xml" not in zf.namelist():
        return []
    root = ET.fromstring(zf.read("xl/sharedStrings.xml"))
    values = []
    for si in root.findall("main:si", NS):
        text = "".join(node.text or "" for node in si.findall(".//main:t", NS))
        values.append(text)
    return values

def workbook_sheet_targets(zf: zipfile.ZipFile) -> list[tuple[str, str]]:
    workbook = ET.fromstring(zf.read("xl/workbook.xml"))
    rel_root = ET.fromstring(zf.read("xl/_rels/workbook.xml.rels"))
    rel_map = {}
    for rel in rel_root.findall("rel:Relationship", NS):
        rel_id = rel.attrib.get("Id")
        target = rel.attrib.get("Target", "")
        if rel_id:
            rel_map[rel_id] = target
    sheets = []
    for sheet in workbook.findall("main:sheets/main:sheet", NS):
        name = sheet.attrib.get("name", "Sheet")
        rel_id = sheet.attrib.get("{http://schemas.openxmlformats.org/officeDocument/2006/relationships}id")
        target = rel_map.get(rel_id, "")
        if target and not target.startswith("xl/"):
            target = f"xl/{target}"
        sheets.append((name, target))
    return sheets

def cell_text(cell: ET.Element, strings: list[str]) -> str:
    cell_type = cell.attrib.get("t", "")
    value = cell.findtext("main:v", default="", namespaces=NS)
    if cell_type == "s" and value.isdigit():
        idx = int(value)
        if 0 <= idx < len(strings):
            return strings[idx]
    if cell_type == "inlineStr":
        return "".join(node.text or "" for node in cell.findall(".//main:t", NS))
    return value

def inspect_xlsx(path: Path, preview_rows: int) -> dict:
    with zipfile.ZipFile(path) as zf:
        strings = shared_strings(zf)
        sheets = []
        for name, target in workbook_sheet_targets(zf):
            if not target or target not in zf.namelist():
                continue
            root = ET.fromstring(zf.read(target))
            rows = []
            for row in root.findall(".//main:sheetData/main:row", NS):
                values = [cell_text(cell, strings) for cell in row.findall("main:c", NS)]
                rows.append(values)
            headers = rows[0] if rows else []
            preview = rows[1:1 + preview_rows] if len(rows) > 1 else []
            sheets.append({
                "name": name,
                "headers": headers,
                "row_count": max(len(rows) - 1, 0),
                "preview_rows": preview,
            })
    return {"kind": "xlsx", "sheets": sheets}

def main() -> None:
    path = Path(sys.argv[1])
    preview_rows = int(sys.argv[2]) if len(sys.argv) > 2 else 5
    ext = path.suffix.lower()
    if ext == ".csv":
        result = inspect_csv(path, preview_rows)
    elif ext == ".xlsx":
        result = inspect_xlsx(path, preview_rows)
    else:
        result = {"error": f"Unsupported tabular format: {ext}"}
    print(json.dumps(result, ensure_ascii=False))

if __name__ == "__main__":
    main()
"#;

fn resolve_path(base_dir: &Path, raw_path: &str) -> PathBuf {
    let path = PathBuf::from(raw_path);
    if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    }
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create '{}': {}", parent.display(), err))?;
    }
    Ok(())
}

fn write_helper_script(workdir: &Path, name: &str, script: &str) -> Result<PathBuf, String> {
    let scripts_dir = workdir.join("codes");
    std::fs::create_dir_all(&scripts_dir).map_err(|err| {
        format!(
            "Failed to create helper dir '{}': {}",
            scripts_dir.display(),
            err
        )
    })?;
    let path = scripts_dir.join(name);
    let mut file = std::fs::File::create(&path).map_err(|err| {
        format!(
            "Failed to create helper script '{}': {}",
            path.display(),
            err
        )
    })?;
    file.write_all(script.as_bytes()).map_err(|err| {
        format!(
            "Failed to write helper script '{}': {}",
            path.display(),
            err
        )
    })?;
    Ok(path)
}

fn parse_executor_json(result: Value) -> Result<Value, String> {
    let success = result
        .get("success")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !success {
        let stderr = result
            .get("stderr")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        return Err(if stderr.is_empty() {
            "Helper execution failed".to_string()
        } else {
            stderr.to_string()
        });
    }

    let stdout = result
        .get("stdout")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();
    serde_json::from_str(stdout)
        .map_err(|err| format!("Helper returned invalid JSON '{}': {}", stdout, err))
}

fn config_string(doc: &Value, path: &[&str]) -> Option<String> {
    let mut cursor = doc;
    for part in path {
        cursor = cursor.get(*part)?;
    }
    cursor.as_str().map(ToString::to_string)
}

fn is_placeholder(value: &str) -> bool {
    value.trim().is_empty() || value.starts_with("YOUR_")
}

struct ImageConfig {
    provider: String,
    api_key: String,
    model: String,
    endpoint: String,
    size: String,
    background: String,
}

fn image_config_from_doc(doc: &Value) -> ImageConfig {
    let provider = config_string(doc, &["features", "image_generation", "provider"])
        .unwrap_or_else(|| "openai".to_string());
    let endpoint = config_string(doc, &["features", "image_generation", "endpoint"])
        .or_else(|| config_string(doc, &["backends", &provider, "endpoint"]))
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
    let api_key = config_string(doc, &["features", "image_generation", "api_key"])
        .filter(|value| !value.trim().is_empty())
        .or_else(|| config_string(doc, &["backends", &provider, "api_key"]))
        .unwrap_or_default();
    let model = config_string(doc, &["features", "image_generation", "model"])
        .or_else(|| config_string(doc, &["backends", &provider, "model"]))
        .unwrap_or_else(|| "gpt-image-1".to_string());
    let size = config_string(doc, &["features", "image_generation", "size"])
        .unwrap_or_else(|| "1024x1024".to_string());
    let background = config_string(doc, &["features", "image_generation", "background"])
        .unwrap_or_else(|| "auto".to_string());
    ImageConfig {
        provider,
        api_key,
        model,
        endpoint,
        size,
        background,
    }
}

pub async fn generate_image(
    prompt: &str,
    output_path: &str,
    requested_size: Option<&str>,
    background: Option<&str>,
    workdir: &Path,
    llm_doc: &Value,
) -> Value {
    let cfg = image_config_from_doc(llm_doc);
    if is_placeholder(&cfg.api_key) {
        return json!({
            "error": format!(
                "Image generation API key is not configured for provider '{}'",
                cfg.provider
            )
        });
    }

    let target = resolve_path(workdir, output_path);
    if let Err(err) = ensure_parent(&target) {
        return json!({ "error": err });
    }

    let request = json!({
        "model": cfg.model,
        "prompt": prompt,
        "size": requested_size.unwrap_or(&cfg.size),
        "background": background.unwrap_or(&cfg.background),
        "response_format": "b64_json"
    });

    let url = format!("{}/images/generations", cfg.endpoint.trim_end_matches('/'));
    let client = crate::generic::infra::http_client::default_client();
    let response = match client
        .post(url)
        .header("Authorization", format!("Bearer {}", cfg.api_key))
        .json(&request)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return json!({ "error": format!("Image request failed: {}", err) });
        }
    };

    let status = response.status();
    let body = match response.text().await {
        Ok(body) => body,
        Err(err) => {
            return json!({ "error": format!("Failed to read image response: {}", err) });
        }
    };

    if !status.is_success() {
        return json!({
            "error": format!("Image provider returned HTTP {}: {}", status.as_u16(), body)
        });
    }

    let parsed: Value = match serde_json::from_str(&body) {
        Ok(parsed) => parsed,
        Err(err) => {
            return json!({ "error": format!("Invalid image response JSON: {}", err) });
        }
    };

    let image_entry = parsed
        .get("data")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .cloned()
        .unwrap_or_else(|| json!({}));

    let bytes = if let Some(b64) = image_entry.get("b64_json").and_then(|value| value.as_str()) {
        match BASE64_STANDARD.decode(b64) {
            Ok(bytes) => bytes,
            Err(err) => {
                return json!({ "error": format!("Failed to decode image payload: {}", err) });
            }
        }
    } else if let Some(url) = image_entry.get("url").and_then(|value| value.as_str()) {
        match client.get(url).send().await {
            Ok(resp) => match resp.bytes().await {
                Ok(bytes) => bytes.to_vec(),
                Err(err) => {
                    return json!({ "error": format!("Failed to read generated image bytes: {}", err) });
                }
            },
            Err(err) => {
                return json!({ "error": format!("Failed to download generated image: {}", err) });
            }
        }
    } else {
        return json!({ "error": "Image provider response did not contain b64_json or url" });
    };

    if let Err(err) = std::fs::write(&target, &bytes) {
        return json!({
            "error": format!("Failed to save generated image '{}': {}", target.display(), err)
        });
    }

    json!({
        "status": "success",
        "path": target.to_string_lossy().to_string(),
        "size_bytes": bytes.len(),
        "model": cfg.model,
        "provider": cfg.provider,
        "prompt": prompt,
    })
}

pub async fn extract_document_text(
    raw_path: &str,
    output_path: Option<&str>,
    max_chars: Option<usize>,
    workdir: &Path,
) -> Value {
    let source = resolve_path(workdir, raw_path);
    if !source.exists() {
        return json!({ "error": format!("Document not found: {}", source.display()) });
    }

    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let text = match extension.as_str() {
        "txt" | "md" | "json" | "csv" | "rs" | "py" | "js" | "ts" | "toml" | "yaml" | "yml" => {
            match std::fs::read_to_string(&source) {
                Ok(text) => text,
                Err(err) => {
                    return json!({
                        "error": format!("Failed to read '{}': {}", source.display(), err)
                    });
                }
            }
        }
        "pdf" => {
            let script_path =
                match write_helper_script(workdir, "extract-pdf-text.py", PDF_EXTRACTOR_SCRIPT) {
                    Ok(path) => path,
                    Err(err) => return json!({ "error": err }),
                };
            let engine = crate::generic::infra::container_engine::ContainerEngine::new();
            let cwd = workdir.to_string_lossy().to_string();
            let script_arg = script_path.to_string_lossy().to_string();
            let source_arg = source.to_string_lossy().to_string();
            let args = [script_arg.as_str(), source_arg.as_str()];
            let result = match engine
                .execute_oneshot("python3", &args, Some(cwd.as_str()))
                .await
            {
                Ok(value) => value,
                Err(err) => {
                    return json!({ "error": format!("PDF extraction helper failed: {}", err) })
                }
            };
            match parse_executor_json(result) {
                Ok(payload) => payload
                    .get("text")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string(),
                Err(err) => return json!({ "error": err }),
            }
        }
        other => {
            return json!({
                "error": format!(
                    "Unsupported document format '.{}'. Supported: txt, md, json, csv, pdf.",
                    other
                )
            });
        }
    };

    let final_text = if let Some(limit) = max_chars {
        text.chars().take(limit).collect::<String>()
    } else {
        text
    };

    let saved_path = if let Some(output_path) = output_path {
        let target = resolve_path(workdir, output_path);
        if let Err(err) = ensure_parent(&target) {
            return json!({ "error": err });
        }
        if let Err(err) = std::fs::write(&target, &final_text) {
            return json!({
                "error": format!("Failed to write '{}': {}", target.display(), err)
            });
        }
        Some(target.to_string_lossy().to_string())
    } else {
        None
    };

    json!({
        "status": "success",
        "path": source.to_string_lossy().to_string(),
        "output_path": saved_path,
        "char_count": final_text.chars().count(),
        "content": if final_text.chars().count() <= 32_000 {
            Value::String(final_text.clone())
        } else {
            Value::Null
        },
        "text_preview": final_text.chars().take(4000).collect::<String>(),
    })
}

pub async fn inspect_tabular_data(raw_path: &str, preview_rows: usize, workdir: &Path) -> Value {
    let source = resolve_path(workdir, raw_path);
    if !source.exists() {
        return json!({ "error": format!("Tabular file not found: {}", source.display()) });
    }

    let script_path =
        match write_helper_script(workdir, "inspect-tabular-data.py", TABULAR_INSPECTOR_SCRIPT) {
            Ok(path) => path,
            Err(err) => return json!({ "error": err }),
        };
    let engine = crate::generic::infra::container_engine::ContainerEngine::new();
    let cwd = workdir.to_string_lossy().to_string();
    let preview_rows_str = preview_rows.to_string();
    let script_arg = script_path.to_string_lossy().to_string();
    let source_arg = source.to_string_lossy().to_string();
    let args = [
        script_arg.as_str(),
        source_arg.as_str(),
        preview_rows_str.as_str(),
    ];
    let result = match engine
        .execute_oneshot("python3", &args, Some(cwd.as_str()))
        .await
    {
        Ok(value) => value,
        Err(err) => return json!({ "error": format!("Tabular inspector helper failed: {}", err) }),
    };
    match parse_executor_json(result) {
        Ok(payload) => json!({
            "status": "success",
            "path": source.to_string_lossy().to_string(),
            "inspection": payload
        }),
        Err(err) => json!({ "error": err }),
    }
}

pub fn validate_web_search(config_dir: &Path, engine: Option<&str>) -> Value {
    let path = config_dir.join("web_search_config.json");
    let doc = match std::fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(doc) => doc,
            Err(err) => {
                return json!({ "error": format!("Invalid web_search_config.json: {}", err) })
            }
        },
        Err(err) => {
            return json!({ "error": format!("Failed to read '{}': {}", path.display(), err) })
        }
    };

    let requested = engine.map(|value| value.to_ascii_lowercase());
    let default_engine = doc
        .get("default_engine")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let mut reports = Vec::new();
    for key in [
        "naver",
        "google",
        "brave",
        "gemini",
        "grok",
        "kimi",
        "perplexity",
    ] {
        if let Some(requested) = &requested {
            if requested != key {
                continue;
            }
        }
        let cfg = doc.get(key).cloned().unwrap_or_else(|| json!({}));
        let required_fields: &[&str] = match key {
            "naver" => &["client_id", "client_secret"],
            "google" => &["api_key", "search_engine_id"],
            "brave" => &["api_key"],
            "gemini" | "grok" | "perplexity" => &["api_key"],
            "kimi" => &["api_key", "base_url"],
            _ => &[],
        };
        let missing: Vec<String> = required_fields
            .iter()
            .filter_map(|field| {
                let value = cfg
                    .get(*field)
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                if is_placeholder(value) {
                    Some((*field).to_string())
                } else {
                    None
                }
            })
            .collect();
        reports.push(json!({
            "engine": key,
            "ready": missing.is_empty(),
            "missing": missing,
            "is_default": key == default_engine,
        }));
    }

    json!({
        "status": "success",
        "config_path": path.to_string_lossy().to_string(),
        "default_engine": default_engine,
        "engines": reports,
    })
}

fn parse_search_config(config_dir: &Path) -> Result<Value, String> {
    let path = config_dir.join("web_search_config.json");
    let raw = std::fs::read_to_string(&path)
        .map_err(|err| format!("Failed to read '{}': {}", path.display(), err))?;
    serde_json::from_str(&raw).map_err(|err| {
        format!(
            "Invalid web_search_config.json '{}': {}",
            path.display(),
            err
        )
    })
}

async fn fallback_search_google(query: &str, limit: usize, cfg: &Value) -> Result<Value, String> {
    let api_key = cfg
        .get("api_key")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let cx = cfg
        .get("search_engine_id")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if is_placeholder(api_key) || is_placeholder(cx) {
        return Err("Google search is not configured".to_string());
    }

    let client = crate::generic::infra::http_client::default_client();
    let limit_str = limit.to_string();
    let response = client
        .get("https://www.googleapis.com/customsearch/v1")
        .query(&[
            ("q", query),
            ("key", api_key),
            ("cx", cx),
            ("num", limit_str.as_str()),
        ])
        .send()
        .await
        .map_err(|err| format!("Google search failed: {}", err))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| format!("Google response read failed: {}", err))?;
    if !status.is_success() {
        return Err(format!(
            "Google search returned HTTP {}: {}",
            status.as_u16(),
            body
        ));
    }
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|err| format!("Google response parse failed: {}", err))?;
    let results: Vec<Value> = parsed
        .get("items")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .take(limit)
        .map(|item| {
            json!({
                "title": item.get("title").and_then(|value| value.as_str()).unwrap_or(""),
                "snippet": item.get("snippet").and_then(|value| value.as_str()).unwrap_or(""),
                "url": item.get("link").and_then(|value| value.as_str()).unwrap_or(""),
            })
        })
        .collect();
    Ok(json!({"engine": "google", "query": query, "results": results}))
}

async fn fallback_search_brave(query: &str, limit: usize, cfg: &Value) -> Result<Value, String> {
    let api_key = cfg
        .get("api_key")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if is_placeholder(api_key) {
        return Err("Brave search is not configured".to_string());
    }

    let client = crate::generic::infra::http_client::default_client();
    let limit_str = limit.to_string();
    let response = client
        .get("https://api.search.brave.com/res/v1/web/search")
        .query(&[("q", query), ("count", limit_str.as_str())])
        .header("Accept", "application/json")
        .header("X-Subscription-Token", api_key)
        .send()
        .await
        .map_err(|err| format!("Brave search failed: {}", err))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| format!("Brave response read failed: {}", err))?;
    if !status.is_success() {
        return Err(format!(
            "Brave search returned HTTP {}: {}",
            status.as_u16(),
            body
        ));
    }
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|err| format!("Brave response parse failed: {}", err))?;
    let results: Vec<Value> = parsed
        .pointer("/web/results")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .take(limit)
        .map(|item| {
            json!({
                "title": item.get("title").and_then(|value| value.as_str()).unwrap_or(""),
                "snippet": item.get("description").and_then(|value| value.as_str()).unwrap_or(""),
                "url": item.get("url").and_then(|value| value.as_str()).unwrap_or(""),
            })
        })
        .collect();
    Ok(json!({"engine": "brave", "query": query, "results": results}))
}

async fn fallback_search_gemini(query: &str, limit: usize, cfg: &Value) -> Result<Value, String> {
    let api_key = cfg
        .get("api_key")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let model = cfg
        .get("model")
        .and_then(|value| value.as_str())
        .unwrap_or("gemini-2.5-flash");
    if is_placeholder(api_key) {
        return Err("Gemini search is not configured".to_string());
    }

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );
    let body = json!({
        "contents": [{"parts": [{"text": query}]}],
        "tools": [{"google_search": {}}]
    });
    let client = crate::generic::infra::http_client::default_client();
    let response = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|err| format!("Gemini search failed: {}", err))?;
    let status = response.status();
    let raw = response
        .text()
        .await
        .map_err(|err| format!("Gemini response read failed: {}", err))?;
    if !status.is_success() {
        return Err(format!(
            "Gemini search returned HTTP {}: {}",
            status.as_u16(),
            raw
        ));
    }
    let parsed: Value = serde_json::from_str(&raw)
        .map_err(|err| format!("Gemini response parse failed: {}", err))?;
    let content = parsed
        .pointer("/candidates/0/content/parts")
        .and_then(|value| value.as_array())
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(|value| value.as_str()))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let results: Vec<Value> = parsed
        .pointer("/candidates/0/groundingMetadata/groundingChunks")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|chunk| chunk.get("web"))
        .take(limit)
        .map(|web| {
            json!({
                "title": web.get("title").and_then(|value| value.as_str()).unwrap_or(""),
                "snippet": "",
                "url": web.get("uri").and_then(|value| value.as_str()).unwrap_or(""),
            })
        })
        .collect();
    Ok(json!({
        "engine": "gemini",
        "query": query,
        "content": content,
        "results": results,
    }))
}

pub async fn web_search(
    query: &str,
    engine: Option<&str>,
    limit: usize,
    workdir: &Path,
    config_dir: &Path,
) -> Value {
    let normalized_limit = limit.clamp(1, 10);
    let requested_engine = engine.map(|value| value.to_ascii_lowercase());
    let config = match parse_search_config(config_dir) {
        Ok(config) => config,
        Err(err) => return json!({ "error": err }),
    };
    let selected_engine = requested_engine
        .clone()
        .or_else(|| {
            config
                .get("default_engine")
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "google".to_string());

    let cli_args = if let Some(engine) = &requested_engine {
        vec![
            "--query".to_string(),
            query.to_string(),
            "--engine".to_string(),
            engine.to_string(),
        ]
    } else {
        vec!["--query".to_string(), query.to_string()]
    };
    let cli_arg_refs: Vec<&str> = cli_args.iter().map(|value| value.as_str()).collect();
    let cwd = workdir.to_string_lossy().to_string();
    let engine_runner = crate::generic::infra::container_engine::ContainerEngine::new();
    if let Ok(result) = engine_runner
        .execute_oneshot("tizen-web-search-cli", &cli_arg_refs, Some(cwd.as_str()))
        .await
    {
        if let Ok(parsed) = parse_executor_json(result) {
            return json!({
                "status": "success",
                "query": query,
                "result": parsed
            });
        }
    }

    let cfg = config
        .get(&selected_engine)
        .cloned()
        .unwrap_or_else(|| json!({}));
    let fallback = match selected_engine.as_str() {
        "google" => fallback_search_google(query, normalized_limit, &cfg).await,
        "brave" => fallback_search_brave(query, normalized_limit, &cfg).await,
        "gemini" => fallback_search_gemini(query, normalized_limit, &cfg).await,
        other => Err(format!(
            "Search engine '{}' is not available without the native search CLI",
            other
        )),
    };

    match fallback {
        Ok(result) => json!({ "status": "success", "query": query, "result": result }),
        Err(err) => json!({ "error": err }),
    }
}
