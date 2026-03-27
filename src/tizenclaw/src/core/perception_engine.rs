//! Perception engine — processes sensor/screen data for agent awareness.

use serde_json::{json, Value};

pub struct PerceptionEngine {
    last_screen_text: String,
}

impl PerceptionEngine {
    pub fn new() -> Self {
        PerceptionEngine { last_screen_text: String::new() }
    }

    pub fn capture_screen(&mut self) -> Value {
        // Invoke tizenclaw-screen-perceptor CLI
        match std::process::Command::new("/usr/bin/tizenclaw-screen-perceptor")
            .arg("--json")
            .output()
        {
            Ok(output) if output.status.success() => {
                let text = String::from_utf8_lossy(&output.stdout).to_string();
                self.last_screen_text = text.clone();
                match serde_json::from_str::<Value>(&text) {
                    Ok(v) => v,
                    Err(_) => json!({"raw_text": text}),
                }
            }
            _ => json!({"error": "Screen capture unavailable"}),
        }
    }

    pub fn get_last_screen_text(&self) -> &str {
        &self.last_screen_text
    }
}
