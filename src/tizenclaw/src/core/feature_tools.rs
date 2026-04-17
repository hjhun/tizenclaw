use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use regex::Regex;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

const PDF_EXTRACTOR_SCRIPT: &str = r#"#!/usr/bin/env python3
import json
import re
import subprocess
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
        try:
            to_scan = zlib.decompress(stream)
        except Exception:
            to_scan = stream
        for token in re.findall(rb"\((.*?)\)\s*Tj", to_scan, re.S):
            try:
                texts.append(token.decode("latin1", errors="ignore"))
            except Exception:
                pass
        for arr in re.findall(rb"\[(.*?)\]\s*TJ", to_scan, re.S):
            parts = re.findall(rb"\((.*?)\)", arr, re.S)
            joined = "".join(part.decode("latin1", errors="ignore") for part in parts)
            if joined:
                texts.append(joined)
    return "\n".join(texts)

def _extract_with_pdftotext(path: Path) -> str:
    try:
        result = subprocess.run(
            ["pdftotext", "-layout", "-nopgbrk", str(path), "-"],
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="ignore",
        )
    except Exception:
        return ""

    if result.returncode != 0:
        return ""
    return result.stdout or ""

def main() -> None:
    path = Path(sys.argv[1])
    raw = path.read_bytes()
    text = _extract_with_pypdf(path)
    extractor = "pypdf"
    if not text.strip():
        text = _extract_with_pdftotext(path)
        if text.strip():
            extractor = "pdftotext"
    if not text.strip():
        text = _extract_flate_streams(raw)
        if text.strip():
            extractor = "flate_fallback"
    text = text.replace("\\r", "\n")
    text = re.sub(r"[ \t]+", " ", text)
    text = re.sub(r"\n{3,}", "\n\n", text).strip()
    print(json.dumps({
        "status": "success",
        "text": text,
        "char_count": len(text),
        "extractor": extractor
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
MAX_FULL_ROWS = 200

def column_index(cell_ref: str) -> int:
    letters = "".join(ch for ch in cell_ref if ch.isalpha()).upper()
    if not letters:
        return 0
    value = 0
    for ch in letters:
        value = value * 26 + (ord(ch) - ord("A") + 1)
    return max(value - 1, 0)

def grouped_numeric_summaries(headers: list[str], data_rows: list[list[str]]) -> dict:
    grouped = {}
    numeric_columns = []
    for idx, header in enumerate(headers):
        values = []
        for row in data_rows:
            if idx >= len(row):
                continue
            cell = row[idx].replace(",", "").strip()
            if not cell:
                continue
            try:
                values.append(float(cell))
            except ValueError:
                values = []
                break
        if values:
            numeric_columns.append((idx, header))

    for group_idx, group_header in enumerate(headers):
        if any(group_idx == numeric_idx for numeric_idx, _ in numeric_columns):
            continue
        buckets = {}
        for row in data_rows:
            if group_idx >= len(row):
                continue
            group_value = row[group_idx].strip()
            if not group_value:
                continue
            bucket = buckets.setdefault(group_value, {})
            for numeric_idx, numeric_header in numeric_columns:
                if numeric_idx >= len(row):
                    continue
                cell = row[numeric_idx].replace(",", "").strip()
                if not cell:
                    continue
                try:
                    value = float(cell)
                except ValueError:
                    continue
                bucket[numeric_header] = bucket.get(numeric_header, 0.0) + value
        if buckets and len(buckets) <= 50:
            grouped[group_header] = buckets
    return grouped

def inspect_csv(path: Path, preview_rows: int) -> dict:
    with path.open("r", encoding="utf-8", newline="") as fh:
        reader = csv.reader(fh)
        rows = list(reader)
    headers = rows[0] if rows else []
    data_rows = rows[1:] if len(rows) > 1 else []
    preview = data_rows[:preview_rows]
    numeric_summaries = {}
    for idx, header in enumerate(headers):
        values = []
        for row in data_rows:
            if idx >= len(row):
                continue
            cell = row[idx].replace(",", "").strip()
            if not cell:
                continue
            try:
                values.append(float(cell))
            except ValueError:
                values = []
                break
        if values:
            numeric_summaries[header] = {
                "sum": sum(values),
                "min": min(values),
                "max": max(values),
                "count": len(values),
            }
    result = {
        "kind": "csv",
        "sheets": [{
            "name": path.name,
            "headers": headers,
            "row_count": len(data_rows),
            "preview_rows": preview,
            "numeric_summaries": numeric_summaries,
            "grouped_summaries": grouped_numeric_summaries(headers, data_rows),
        }]
    }
    if len(data_rows) <= MAX_FULL_ROWS:
        result["sheets"][0]["rows"] = data_rows
        result["sheets"][0]["rows_truncated"] = False
    else:
        result["sheets"][0]["rows_truncated"] = True
    return result

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
        if target.startswith("/"):
            target = target.lstrip("/")
        elif target and not target.startswith("xl/"):
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
                values = []
                for cell in row.findall("main:c", NS):
                    idx = column_index(cell.attrib.get("r", ""))
                    while len(values) <= idx:
                        values.append("")
                    values[idx] = cell_text(cell, strings)
                rows.append(values)
            headers = rows[0] if rows else []
            data_rows = rows[1:] if len(rows) > 1 else []
            preview = data_rows[:preview_rows]
            numeric_summaries = {}
            for idx, header in enumerate(headers):
                values = []
                for row in data_rows:
                    if idx >= len(row):
                        continue
                    cell = row[idx].replace(",", "").strip()
                    if not cell:
                        continue
                    try:
                        values.append(float(cell))
                    except ValueError:
                        values = []
                        break
                if values:
                    numeric_summaries[header] = {
                        "sum": sum(values),
                        "min": min(values),
                        "max": max(values),
                        "count": len(values),
                    }
            sheet_result = {
                "name": name,
                "headers": headers,
                "row_count": len(data_rows),
                "preview_rows": preview,
                "numeric_summaries": numeric_summaries,
                "grouped_summaries": grouped_numeric_summaries(headers, data_rows),
            }
            if len(data_rows) <= MAX_FULL_ROWS:
                sheet_result["rows"] = data_rows
                sheet_result["rows_truncated"] = False
            else:
                sheet_result["rows_truncated"] = True
            sheets.append(sheet_result)
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

const CALENDAR_MONTH_PATTERN: &str = concat!(
    r"(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|",
    r"jul(?:y)?|aug(?:ust)?|sep(?:t|tember)?|oct(?:ober)?|nov(?:ember)?|",
    r"dec(?:ember)?)\.?"
);

fn specific_calendar_date_regex() -> Option<Regex> {
    Regex::new(&format!(
        r"(?ix)
        \b
        (?:
            {month}
            \s+\d{{1,2}}
            (?:\s*[–-]\s*(?:{month}\s+)?\d{{1,2}})?
            ,\s+\d{{4}}
            |
            \d{{4}}-\d{{2}}-\d{{2}}
        )
        \b",
        month = CALENDAR_MONTH_PATTERN,
    ))
    .ok()
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
    let trimmed = value.trim();
    trimmed.is_empty()
        || trimmed.starts_with("YOUR_")
        || trimmed.eq_ignore_ascii_case("***REDACTED***")
        || trimmed.eq_ignore_ascii_case("REDACTED")
        || trimmed.contains("<redacted>")
}

fn search_engine_required_fields(engine: &str) -> &'static [&'static str] {
    match engine {
        "naver" => &["client_id", "client_secret"],
        "google" => &["api_key", "search_engine_id"],
        "brave" => &["api_key"],
        "gemini" | "grok" | "perplexity" => &["api_key"],
        "kimi" => &["api_key", "base_url"],
        _ => &[],
    }
}

fn search_engine_ready(doc: &Value, engine: &str) -> bool {
    if matches!(engine, "duckduckgo" | "duckduckgo_mirror") {
        return true;
    }

    let cfg = doc.get(engine).cloned().unwrap_or_else(|| json!({}));
    search_engine_required_fields(engine).iter().all(|field| {
        let value = cfg
            .get(*field)
            .and_then(|value| value.as_str())
            .unwrap_or("");
        !is_placeholder(value)
    })
}

fn resolve_search_engine(doc: &Value, requested_engine: Option<&str>) -> String {
    if let Some(requested) = requested_engine
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
    {
        return requested;
    }

    let default_engine = doc
        .get("default_engine")
        .and_then(|value| value.as_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| "duckduckgo_mirror".to_string());

    if search_engine_ready(doc, &default_engine) {
        default_engine
    } else {
        "duckduckgo_mirror".to_string()
    }
}

fn parse_requested_image_size(value: &str) -> (u32, u32) {
    let normalized = value.trim().to_ascii_lowercase();
    let Some((w, h)) = normalized.split_once('x') else {
        return (1024, 1024);
    };
    let width = w.parse::<u32>().ok().filter(|value| *value >= 128).unwrap_or(1024);
    let height = h.parse::<u32>().ok().filter(|value| *value >= 128).unwrap_or(1024);
    (width.min(1536), height.min(1536))
}

fn paint_pixel(buffer: &mut [u8], width: u32, height: u32, x: u32, y: u32, color: [u8; 4]) {
    if x >= width || y >= height {
        return;
    }
    let idx = ((y * width + x) * 4) as usize;
    if let Some(slice) = buffer.get_mut(idx..idx + 4) {
        slice.copy_from_slice(&color);
    }
}

fn draw_filled_rect(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    color: [u8; 4],
) {
    let left = x0.min(width);
    let top = y0.min(height);
    let right = x1.min(width);
    let bottom = y1.min(height);
    for y in top..bottom {
        for x in left..right {
            paint_pixel(canvas, width, height, x, y, color);
        }
    }
}

fn draw_circle(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    center_x: i32,
    center_y: i32,
    radius: i32,
    color: [u8; 4],
) {
    let width = width as i32;
    let height = height as i32;
    let left = (center_x - radius).max(0);
    let right = (center_x + radius).min(width - 1);
    let top = (center_y - radius).max(0);
    let bottom = (center_y + radius).min(height - 1);
    let radius_sq = radius * radius;
    for y in top..=bottom {
        for x in left..=right {
            let dx = x - center_x;
            let dy = y - center_y;
            if dx * dx + dy * dy <= radius_sq {
                paint_pixel(canvas, width as u32, height as u32, x as u32, y as u32, color);
            }
        }
    }
}

fn adler32(bytes: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65_521;
    let mut a = 1u32;
    let mut b = 0u32;
    for byte in bytes {
        a = (a + *byte as u32) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }
    (b << 16) | a
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg() & 0xEDB8_8320;
            crc = (crc >> 1) ^ mask;
        }
    }
    !crc
}

fn write_png_rgba(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let row_bytes = width as usize * 4;
    if rgba.len() != row_bytes * height as usize {
        return Err("RGBA buffer length does not match requested PNG size".to_string());
    }

    let mut filtered = Vec::with_capacity((row_bytes + 1) * height as usize);
    for row in rgba.chunks_exact(row_bytes) {
        filtered.push(0);
        filtered.extend_from_slice(row);
    }

    let mut zlib = Vec::with_capacity(filtered.len() + 64);
    zlib.extend_from_slice(&[0x78, 0x01]);
    let mut remaining = filtered.as_slice();
    while !remaining.is_empty() {
        let block_len = remaining.len().min(65_535);
        let (block, tail) = remaining.split_at(block_len);
        let is_final = tail.is_empty();
        zlib.push(if is_final { 0x01 } else { 0x00 });
        let len_bytes = (block_len as u16).to_le_bytes();
        let nlen_bytes = (!(block_len as u16)).to_le_bytes();
        zlib.extend_from_slice(&len_bytes);
        zlib.extend_from_slice(&nlen_bytes);
        zlib.extend_from_slice(block);
        remaining = tail;
    }
    zlib.extend_from_slice(&adler32(&filtered).to_be_bytes());

    let mut out = Vec::new();
    out.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    let mut write_chunk = |name: &[u8; 4], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(data);
        let mut crc_payload = Vec::with_capacity(name.len() + data.len());
        crc_payload.extend_from_slice(name);
        crc_payload.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc_payload).to_be_bytes());
    };

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    write_chunk(b"IHDR", &ihdr);
    write_chunk(b"IDAT", &zlib);
    write_chunk(b"IEND", &[]);

    std::fs::write(path, out)
        .map_err(|err| format!("Failed to write fallback PNG '{}': {}", path.display(), err))
}

fn render_lightweight_scene_fallback(
    prompt: &str,
    output_path: &Path,
    requested_size: Option<&str>,
) -> Result<Value, String> {
    let (width, height) = parse_requested_image_size(requested_size.unwrap_or("1024x1024"));
    let cozy_prompt = prompt.to_ascii_lowercase();
    let warm_top = if cozy_prompt.contains("cozy") || cozy_prompt.contains("coffee") {
        [245, 224, 196, 255]
    } else {
        [223, 233, 242, 255]
    };
    let warm_bottom = if cozy_prompt.contains("night") {
        [88, 61, 41, 255]
    } else {
        [197, 152, 112, 255]
    };
    let mut canvas = vec![0u8; width as usize * height as usize * 4];
    for y in 0..height {
        let ratio = y as f32 / height.max(1) as f32;
        let blend = |a: u8, b: u8| ((a as f32 * (1.0 - ratio)) + (b as f32 * ratio)) as u8;
        let color = [
            blend(warm_top[0], warm_bottom[0]),
            blend(warm_top[1], warm_bottom[1]),
            blend(warm_top[2], warm_bottom[2]),
            255,
        ];
        for x in 0..width {
            paint_pixel(&mut canvas, width, height, x, y, color);
        }
    }

    let floor_y = height * 3 / 4;
    draw_filled_rect(
        &mut canvas,
        width,
        height,
        0,
        floor_y,
        width,
        height,
        [111, 78, 55, 255],
    );
    draw_filled_rect(
        &mut canvas,
        width,
        height,
        width / 10,
        height / 5,
        width / 4,
        floor_y - height / 10,
        [108, 78, 57, 255],
    );
    draw_filled_rect(
        &mut canvas,
        width,
        height,
        width / 8,
        height / 4,
        width / 4 - width / 30,
        height / 3,
        [150, 116, 84, 255],
    );
    draw_filled_rect(
        &mut canvas,
        width,
        height,
        width / 3,
        height / 2,
        width * 4 / 5,
        height / 2 + height / 20,
        [92, 63, 45, 255],
    );
    draw_filled_rect(
        &mut canvas,
        width,
        height,
        width * 3 / 5,
        height / 2 + height / 20,
        width * 3 / 5 + width / 35,
        floor_y,
        [74, 50, 36, 255],
    );

    let robot_center_x = width as i32 / 2;
    let robot_head_y = height as i32 / 3;
    draw_circle(
        &mut canvas,
        width,
        height,
        robot_center_x,
        robot_head_y,
        (width.min(height) / 10) as i32,
        [189, 201, 214, 255],
    );
    draw_circle(
        &mut canvas,
        width,
        height,
        robot_center_x - (width as i32 / 35),
        robot_head_y - (height as i32 / 80),
        (width.min(height) / 55) as i32,
        [55, 73, 91, 255],
    );
    draw_circle(
        &mut canvas,
        width,
        height,
        robot_center_x + (width as i32 / 35),
        robot_head_y - (height as i32 / 80),
        (width.min(height) / 55) as i32,
        [55, 73, 91, 255],
    );
    draw_filled_rect(
        &mut canvas,
        width,
        height,
        width / 2 - width / 12,
        height / 3 + height / 18,
        width / 2 + width / 12,
        height * 11 / 18,
        [168, 183, 197, 255],
    );
    draw_filled_rect(
        &mut canvas,
        width,
        height,
        width / 2 - width / 18,
        height * 11 / 18,
        width / 2 + width / 18,
        floor_y,
        [86, 103, 120, 255],
    );
    draw_filled_rect(
        &mut canvas,
        width,
        height,
        width / 2 + width / 16,
        height / 2,
        width / 2 + width / 5,
        height / 2 + height / 30,
        [168, 183, 197, 255],
    );

    if cozy_prompt.contains("book") || cozy_prompt.contains("reading") || cozy_prompt.contains("read")
    {
        draw_filled_rect(
            &mut canvas,
            width,
            height,
            width / 2 + width / 7,
            height / 2 - height / 25,
            width / 2 + width / 4,
            height / 2 + height / 18,
            [98, 59, 132, 255],
        );
        draw_filled_rect(
            &mut canvas,
            width,
            height,
            width / 2 + width / 7 + width / 150,
            height / 2 - height / 25 + height / 150,
            width / 2 + width / 4 - width / 150,
            height / 2 + height / 18 - height / 150,
            [244, 236, 215, 255],
        );
    }

    if cozy_prompt.contains("coffee") || cozy_prompt.contains("cafe") || cozy_prompt.contains("shop") {
        draw_filled_rect(
            &mut canvas,
            width,
            height,
            width * 11 / 20,
            height / 2 - height / 18,
            width * 11 / 20 + width / 30,
            height / 2 + height / 36,
            [233, 236, 241, 255],
        );
        draw_circle(
            &mut canvas,
            width,
            height,
            (width * 11 / 20 + width / 24) as i32,
            (height / 2 - height / 24) as i32,
            (width.min(height) / 60) as i32,
            [99, 58, 33, 255],
        );
    }

    let ext = output_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => write_png_rgba(output_path, width, height, &canvas)?,
        other => {
            return Err(format!(
                "Local fallback image renderer does not support '.{}' outputs yet",
                other
            ))
        }
    }

    Ok(json!({
        "status": "success",
        "provider": "local_fallback",
        "path": output_path.to_string_lossy().to_string(),
        "prompt": prompt,
        "size": format!("{}x{}", width, height)
    }))
}

fn percent_decode(value: &str) -> String {
    let mut out = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        match bytes[idx] {
            b'%' if idx + 2 < bytes.len() => {
                let hex = &value[idx + 1..idx + 3];
                if let Ok(decoded) = u8::from_str_radix(hex, 16) {
                    out.push(decoded);
                    idx += 3;
                    continue;
                }
                out.push(bytes[idx]);
                idx += 1;
            }
            b'+' => {
                out.push(b' ');
                idx += 1;
            }
            other => {
                out.push(other);
                idx += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn decode_duckduckgo_redirect(url: &str) -> String {
    if let Some(pos) = url.find("uddg=") {
        let remainder = &url[pos + 5..];
        let encoded = remainder.split('&').next().unwrap_or(remainder);
        return percent_decode(encoded);
    }
    url.to_string()
}

fn strip_markdown_markup(text: &str) -> String {
    text.replace("**", "")
        .replace("__", "")
        .replace('`', "")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .trim()
        .to_string()
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_html_excerpt(body: &str) -> String {
    let meta_description = Regex::new(
        r#"(?is)<meta[^>]+name=["']description["'][^>]+content=["']([^"']+)["']"#,
    )
    .ok()
    .and_then(|re| re.captures(body))
    .and_then(|captures| captures.get(1).map(|value| collapse_whitespace(value.as_str())))
    .filter(|value| !value.is_empty());
    if let Some(description) = meta_description {
        return description;
    }

    let without_scripts = Regex::new(r"(?is)<script[^>]*>.*?</script>|<style[^>]*>.*?</style>")
        .map(|re| re.replace_all(body, " ").into_owned())
        .unwrap_or_else(|_| body.to_string());
    let text = Regex::new(r"(?is)<[^>]+>")
        .map(|re| re.replace_all(&without_scripts, " ").into_owned())
        .unwrap_or(without_scripts);
    let compact = collapse_whitespace(&text);
    if compact.is_empty() {
        return String::new();
    }

    let date_re = specific_calendar_date_regex();
    if let Some(re) = date_re {
        if let Some(matched) = re.find(&compact) {
            let matched_prefix_chars = compact[..matched.start()].chars().count();
            let matched_chars = compact[matched.start()..matched.end()].chars().count();
            let window_start = matched_prefix_chars.saturating_sub(120);
            let window_end = (matched_prefix_chars + matched_chars + 180)
                .min(compact.chars().count());
            return compact
                .chars()
                .skip(window_start)
                .take(window_end.saturating_sub(window_start))
                .collect::<String>()
                .trim()
                .to_string();
        }
    }

    compact.chars().take(320).collect::<String>().trim().to_string()
}

fn text_contains_specific_date(value: &str) -> bool {
    specific_calendar_date_regex()
        .map(|re| re.is_match(value))
        .unwrap_or(false)
}

fn choose_search_result_snippet(snippet: &str, excerpt: &str) -> String {
    let snippet = snippet.trim();
    let excerpt = excerpt.trim();
    if snippet.is_empty() {
        return excerpt.to_string();
    }
    if excerpt.is_empty() {
        return snippet.to_string();
    }
    if text_contains_specific_date(excerpt) && !text_contains_specific_date(snippet) {
        return excerpt.to_string();
    }

    let snippet_words = snippet.split_whitespace().count();
    let excerpt_words = excerpt.split_whitespace().count();
    if excerpt_words >= snippet_words + 6 {
        return excerpt.to_string();
    }

    snippet.to_string()
}

const SEARCH_RESULT_EXCERPT_TIMEOUT_SECS: u64 = 5;
const SEARCH_RESULT_EXCERPT_FETCH_LIMIT: usize = 5;

fn search_result_quality_score(title: &str, snippet: &str, url: &str) -> i32 {
    let title_lower = title.to_ascii_lowercase();
    let snippet_lower = snippet.to_ascii_lowercase();
    let url_lower = url.to_ascii_lowercase();

    let mut score = 0;
    if text_contains_specific_date(title) || text_contains_specific_date(snippet) {
        score += 5;
    }
    if snippet_lower.contains("official") {
        score += 2;
    }
    if url_lower.starts_with("https://") {
        score += 1;
    }
    if !snippet.trim().is_empty() {
        score += 1;
    }

    let ancillary_markers = [
        "/session/",
        "/sessions/",
        "/on-demand/",
        "/news",
        "/blog",
        "/article",
        "/terms",
        "/privacy",
        "/legal",
        "/cookies",
        "/cookie",
        "/tickets",
        "/ticket",
        "/investor-pass",
        "/exhibit",
        "/watch",
        "/keynotes",
        "/support/",
    ];
    if ancillary_markers
        .iter()
        .any(|marker| url_lower.contains(marker))
    {
        score -= 8;
    }

    let weak_title_markers = [
        "latest news",
        "tickets",
        "investor pass",
        "watch on demand",
        "session catalog",
        "event overview",
        "keynote",
        "blog",
    ];
    if weak_title_markers
        .iter()
        .any(|marker| title_lower.contains(marker))
    {
        score -= 5;
    }

    if url_lower.contains('?') {
        score -= 2;
    }

    score
}

async fn fetch_search_result_excerpt(url: &str) -> Option<String> {
    let client = crate::generic::infra::http_client::default_client();
    let response = client
        .get(url)
        .timeout(std::time::Duration::from_secs(
            SEARCH_RESULT_EXCERPT_TIMEOUT_SECS,
        ))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body = response.text().await.ok()?;
    let excerpt = extract_html_excerpt(&body);
    if excerpt.is_empty() {
        None
    } else {
        Some(excerpt)
    }
}

async fn fallback_search_duckduckgo(query: &str, limit: usize) -> Result<Value, String> {
    let encoded_query = percent_encode_query(query);
    let url = format!(
        "https://r.jina.ai/http://html.duckduckgo.com/html/?q={}",
        encoded_query
    );
    let client = crate::generic::infra::http_client::default_client();
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| format!("DuckDuckGo mirror search failed: {}", err))?;
    let status = response.status();
    let raw = response
        .text()
        .await
        .map_err(|err| format!("DuckDuckGo mirror response read failed: {}", err))?;
    if !status.is_success() {
        return Err(format!(
            "DuckDuckGo mirror returned HTTP {}: {}",
            status.as_u16(),
            raw
        ));
    }

    let heading_re = Regex::new(r"(?m)^## \[(?P<title>[^\]]+)\]\((?P<url>[^)]+)\)\s*$")
        .map_err(|err| format!("DuckDuckGo parser regex failed: {}", err))?;
    let mut matches = heading_re.captures_iter(&raw).peekable();
    let mut results = Vec::new();
    let mut excerpt_fetches = 0usize;
    while let Some(capture) = matches.next() {
        let full = capture.get(0).ok_or_else(|| "Missing match body".to_string())?;
        let start = full.end();
        let end = matches
            .peek()
            .and_then(|next| next.get(0).map(|entry| entry.start()))
            .unwrap_or(raw.len());
        let block = &raw[start..end];
        let snippet = block
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .filter(|line| !line.starts_with('['))
            .filter(|line| !line.starts_with("![]"))
            .filter(|line| !line.starts_with("URL Source:"))
            .filter(|line| !line.starts_with("Markdown Content:"))
            .map(strip_markdown_markup)
            .find(|line| !line.is_empty())
            .unwrap_or_default();
        let title = strip_markdown_markup(capture.name("title").map(|m| m.as_str()).unwrap_or(""));
        let url = decode_duckduckgo_redirect(capture.name("url").map(|m| m.as_str()).unwrap_or(""));
        let excerpt = if (snippet.is_empty() || !text_contains_specific_date(&snippet))
            && excerpt_fetches < SEARCH_RESULT_EXCERPT_FETCH_LIMIT
        {
            excerpt_fetches += 1;
            fetch_search_result_excerpt(&url).await.unwrap_or_default()
        } else {
            String::new()
        };
        let snippet = choose_search_result_snippet(&snippet, &excerpt);
        results.push(json!({
            "title": title,
            "snippet": snippet,
            "url": url,
        }));
    }

    if results.is_empty() {
        return Err("DuckDuckGo mirror returned no parsable results".to_string());
    }

    results.sort_by(|left, right| {
        let left_title = left.get("title").and_then(|value| value.as_str()).unwrap_or("");
        let left_snippet = left
            .get("snippet")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let left_url = left.get("url").and_then(|value| value.as_str()).unwrap_or("");
        let right_title = right
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let right_snippet = right
            .get("snippet")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let right_url = right.get("url").and_then(|value| value.as_str()).unwrap_or("");

        search_result_quality_score(right_title, right_snippet, right_url).cmp(
            &search_result_quality_score(left_title, left_snippet, left_url),
        )
    });
    results.truncate(limit);

    Ok(json!({
        "engine": "duckduckgo_mirror",
        "query": query,
        "results": results
    }))
}

fn percent_encode_query(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            b' ' => encoded.push_str("%20"),
            _ => encoded.push_str(&format!("%{:02X}", byte)),
        }
    }
    encoded
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
    let mut api_key = config_string(doc, &["features", "image_generation", "api_key"])
        .filter(|value| !value.trim().is_empty())
        .or_else(|| config_string(doc, &["backends", &provider, "api_key"]))
        .unwrap_or_default();
    if provider == "openai" && is_placeholder(&api_key) {
        api_key = config_string(doc, &["backends", "openai-codex", "oauth", "access_token"])
            .filter(|value| !is_placeholder(value))
            .unwrap_or_default();
    }
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
    let target = resolve_path(workdir, output_path);
    if let Err(err) = ensure_parent(&target) {
        return json!({ "error": err });
    }

    if is_placeholder(&cfg.api_key) {
        return match render_lightweight_scene_fallback(prompt, &target, requested_size) {
            Ok(result) => result,
            Err(err) => json!({
                "error": format!(
                    "Image generation API key is not configured for provider '{}' and local fallback failed: {}",
                    cfg.provider, err
                )
            }),
        };
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
        return match render_lightweight_scene_fallback(prompt, &target, requested_size) {
            Ok(result) => result,
            Err(err) => json!({
                "error": format!(
                    "Image provider returned HTTP {}: {} (local fallback failed: {})",
                    status.as_u16(),
                    body,
                    err
                )
            }),
        };
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

    let requested = engine
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
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
        let missing: Vec<String> = search_engine_required_fields(key)
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
    if requested.as_deref().map(|value| value == "duckduckgo" || value == "duckduckgo_mirror")
        != Some(false)
    {
        reports.push(json!({
            "engine": "duckduckgo_mirror",
            "ready": true,
            "missing": [],
            "is_default": default_engine.eq_ignore_ascii_case("duckduckgo")
                || default_engine.eq_ignore_ascii_case("duckduckgo_mirror"),
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
    let requested_engine = engine
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    let config = match parse_search_config(config_dir) {
        Ok(config) => config,
        Err(err) => return json!({ "error": err }),
    };
    let selected_engine = resolve_search_engine(&config, requested_engine.as_deref());

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
        "duckduckgo" | "duckduckgo_mirror" => fallback_search_duckduckgo(query, normalized_limit).await,
        other => Err(format!(
            "Search engine '{}' is not available without the native search CLI",
            other
        )),
    };

    match fallback {
        Ok(result) => json!({ "status": "success", "query": query, "result": result }),
        Err(err) => match fallback_search_duckduckgo(query, normalized_limit).await {
            Ok(result) => json!({
                "status": "success",
                "query": query,
                "result": result,
                "fallback_reason": err
            }),
            Err(duck_err) => json!({
                "error": format!("{} | DuckDuckGo fallback failed: {}", err, duck_err)
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parse_requested_image_size_clamps_invalid_values() {
        assert_eq!(parse_requested_image_size("256x512"), (256, 512));
        assert_eq!(parse_requested_image_size("oops"), (1024, 1024));
        assert_eq!(parse_requested_image_size("8x99999"), (1024, 1536));
    }

    #[test]
    fn resolve_search_engine_uses_duckduckgo_when_default_is_unconfigured() {
        let config = json!({
            "default_engine": "naver",
            "naver": {
                "client_id": "YOUR_NAVER_CLIENT_ID",
                "client_secret": "YOUR_NAVER_CLIENT_SECRET"
            }
        });

        assert_eq!(
            resolve_search_engine(&config, None),
            "duckduckgo_mirror".to_string()
        );
    }

    #[test]
    fn duckduckgo_redirect_decoder_extracts_actual_url() {
        let url = "http://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fevent%3Fa%3D1%26b%3D2&rut=abc";
        assert_eq!(
            decode_duckduckgo_redirect(url),
            "https://example.com/event?a=1&b=2"
        );
    }

    #[test]
    fn extract_html_excerpt_prefers_meta_description_and_dates() {
        let html = r#"
        <html>
          <head>
            <meta name="description" content="Conference date: April 22-24, 2026 in Las Vegas." />
          </head>
          <body>
            <p>Ignore this body text.</p>
          </body>
        </html>
        "#;

        let excerpt = extract_html_excerpt(html);
        assert!(excerpt.contains("April 22-24, 2026"));
        assert!(excerpt.contains("Las Vegas"));
    }

    #[test]
    fn extract_html_excerpt_keeps_utf8_boundaries_around_date_windows() {
        let html = r#"
        <html>
          <body>
            <p>Top Cybersecurity Conferences – Events to Attend in 2026.</p>
            <p>Black Hat USA 2026 runs August 1-6, 2026 in Las Vegas, Nevada.</p>
          </body>
        </html>
        "#;

        let excerpt = extract_html_excerpt(html);
        assert!(excerpt.contains("August 1-6, 2026"));
        assert!(excerpt.contains("Las Vegas"));
    }

    #[test]
    fn choose_search_result_snippet_prefers_excerpt_with_specific_date() {
        let snippet = "Annual conference in Las Vegas.";
        let excerpt = "Annual conference in Las Vegas on April 22-24, 2026.";

        assert_eq!(
            choose_search_result_snippet(snippet, excerpt),
            excerpt.to_string()
        );
    }

    #[test]
    fn choose_search_result_snippet_keeps_existing_specific_snippet() {
        let snippet = "Conference date: April 22-24, 2026 in Las Vegas.";
        let excerpt = "Conference page with speaker details and venue maps.";

        assert_eq!(
            choose_search_result_snippet(snippet, excerpt),
            snippet.to_string()
        );
    }

    #[test]
    fn text_contains_specific_date_accepts_abbreviated_cross_month_ranges() {
        assert!(text_contains_specific_date("AWS re:Invent runs Nov. 30-Dec. 4, 2026."));
        assert!(text_contains_specific_date("VMware Explore is Aug. 31-Sep. 3, 2026."));
    }

    #[test]
    fn search_result_quality_score_prefers_primary_event_page() {
        let landing_score = search_result_quality_score(
            "Microsoft Build, June 2-3, 2026 / San Francisco and online",
            "Go deep on real code and real systems with the teams building and scaling AI.",
            "https://build.microsoft.com/en-US/home",
        );
        let ticket_score = search_result_quality_score(
            "TechCrunch Disrupt 2026 - Tickets",
            "TechCrunch Disrupt 2026 is the startup epicenter for tech and VC leaders.",
            "https://techcrunch.com/events/tc-disrupt-2026/tickets/",
        );

        assert!(landing_score > ticket_score);
    }

    #[test]
    fn search_result_quality_score_penalizes_session_pages() {
        let event_score = search_result_quality_score(
            "NVIDIA GTC 2026 | March 16-19, 2026",
            "NVIDIA GTC 2026 will take place March 16-19, 2026 in San Jose, California.",
            "https://www.nvidia.com/gtc/",
        );
        let session_score = search_result_quality_score(
            "GTC 2026 Keynote S81595 | GTC San Jose 2026 | NVIDIA On-Demand",
            "In this keynote, NVIDIA founder and CEO Jensen Huang looks ahead to the future.",
            "https://www.nvidia.com/en-us/on-demand/session/gtc26-s81595/",
        );

        assert!(event_score > session_score);
    }

    #[test]
    fn search_result_quality_score_penalizes_legal_pages() {
        let landing_score = search_result_quality_score(
            "Google I/O 2026 | May 19-20, 2026",
            "Official event page for Google I/O 2026 in Mountain View.",
            "https://io.google/2026/",
        );
        let legal_score = search_result_quality_score(
            "Google I/O 2026 Terms",
            "Terms for the Google I/O event.",
            "https://developers.google.com/events/io/2026/terms",
        );

        assert!(landing_score > legal_score);
    }

    #[test]
    fn lightweight_scene_fallback_writes_valid_png() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("robot_cafe.png");
        let result = render_lightweight_scene_fallback(
            "friendly robot in a cozy coffee shop reading a book",
            &output,
            Some("512x512"),
        )
        .unwrap();
        assert_eq!(result["status"], "success");
        assert!(output.exists());
        let bytes = std::fs::read(&output).unwrap();
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
    }

    #[tokio::test]
    async fn inspect_tabular_data_returns_full_rows_for_small_csv() {
        let dir = tempdir().unwrap();
        let csv_path = dir.path().join("sales.csv");
        std::fs::write(
            &csv_path,
            "region,revenue\nNorth,10\nSouth,20\nEast,30\n",
        )
        .unwrap();

        let result = inspect_tabular_data("sales.csv", 2, dir.path()).await;
        assert_eq!(result["status"], "success");
        assert_eq!(result["inspection"]["kind"], "csv");
        assert_eq!(result["inspection"]["sheets"][0]["row_count"], 3);
        assert_eq!(result["inspection"]["sheets"][0]["rows_truncated"], false);
        assert_eq!(result["inspection"]["sheets"][0]["rows"][2][0], "East");
        assert_eq!(result["inspection"]["sheets"][0]["rows"][2][1], "30");
        assert_eq!(
            result["inspection"]["sheets"][0]["numeric_summaries"]["revenue"]["sum"],
            60.0
        );
        assert_eq!(
            result["inspection"]["sheets"][0]["grouped_summaries"]["region"]["East"]["revenue"],
            30.0
        );
    }
}
