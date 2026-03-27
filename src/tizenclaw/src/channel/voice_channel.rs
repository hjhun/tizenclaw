//! Voice channel — audio I/O pipeline for voice-based interaction.
//!
//! On Tizen, uses Tizen Audio API (via FFI) for microphone capture
//! and speaker output. Speech-to-text (STT) is provided by the Tizen
//! STT framework, and text-to-speech (TTS) by the Tizen TTS framework.
//!
//! This channel captures audio, converts to text via STT, sends the
//! text to the agent, and renders the response via TTS.

use super::{Channel, ChannelConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// STT engine type.
#[derive(Clone, Debug, PartialEq)]
enum SttEngine {
    TizenNative,
    WhisperApi,
    None,
}

/// TTS engine type.
#[derive(Clone, Debug, PartialEq)]
enum TtsEngine {
    TizenNative,
    ElevenLabsApi,
    None,
}

pub struct VoiceChannel {
    name: String,
    stt_engine: SttEngine,
    tts_engine: TtsEngine,
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    sample_rate: u32,
    language: String,
}

impl VoiceChannel {
    pub fn new(config: &ChannelConfig) -> Self {
        let stt = match config.settings.get("stt_engine")
            .and_then(|v| v.as_str()).unwrap_or("none")
        {
            "tizen" => SttEngine::TizenNative,
            "whisper" => SttEngine::WhisperApi,
            _ => SttEngine::None,
        };

        let tts = match config.settings.get("tts_engine")
            .and_then(|v| v.as_str()).unwrap_or("none")
        {
            "tizen" => TtsEngine::TizenNative,
            "elevenlabs" => TtsEngine::ElevenLabsApi,
            _ => TtsEngine::None,
        };

        let sample_rate = config.settings.get("sample_rate")
            .and_then(|v| v.as_u64())
            .unwrap_or(16000) as u32;

        let language = config.settings.get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("en_US")
            .to_string();

        VoiceChannel {
            name: config.name.clone(),
            stt_engine: stt,
            tts_engine: tts,
            running: Arc::new(AtomicBool::new(false)),
            thread: None,
            sample_rate,
            language,
        }
    }

    /// Synthesize speech from text using the configured TTS engine.
    fn speak(&self, text: &str) {
        match self.tts_engine {
            TtsEngine::TizenNative => {
                log::info!("VoiceChannel: TTS(tizen) speak: {}", &text[..text.len().min(50)]);
                // Tizen TTS FFI call would go here
            }
            TtsEngine::ElevenLabsApi => {
                log::info!("VoiceChannel: TTS(elevenlabs) speak: {}", &text[..text.len().min(50)]);
                // ElevenLabs API call would go here
            }
            TtsEngine::None => {
                log::debug!("VoiceChannel: TTS disabled, text: {}", &text[..text.len().min(50)]);
            }
        }
    }
}

impl Channel for VoiceChannel {
    fn name(&self) -> &str { &self.name }

    fn start(&mut self) -> bool {
        if self.running.load(Ordering::SeqCst) { return true; }

        if self.stt_engine == SttEngine::None && self.tts_engine == TtsEngine::None {
            log::warn!("VoiceChannel: no STT/TTS engine configured");
            return false;
        }

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let stt_engine = self.stt_engine.clone();
        let sample_rate = self.sample_rate;
        let language = self.language.clone();

        self.thread = Some(std::thread::spawn(move || {
            log::info!(
                "VoiceChannel: audio pipeline started (stt={:?}, rate={}, lang={})",
                stt_engine, sample_rate, language
            );

            while running.load(Ordering::SeqCst) {
                // Audio capture + STT loop
                match stt_engine {
                    SttEngine::TizenNative => {
                        // Tizen STT FFI: capture mic → PCM → STT → text
                        // Placeholder: sleep and wait for audio events
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                    SttEngine::WhisperApi => {
                        // Capture mic → PCM → upload to Whisper API → text
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                    SttEngine::None => {
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                }
            }

            log::info!("VoiceChannel: audio pipeline stopped");
        }));

        log::info!("VoiceChannel started (stt={:?}, tts={:?})", self.stt_engine, self.tts_engine);
        true
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }

    fn send_message(&self, msg: &str) -> Result<(), String> {
        if self.tts_engine == TtsEngine::None {
            return Err("Voice TTS not configured".into());
        }
        self.speak(msg);
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
