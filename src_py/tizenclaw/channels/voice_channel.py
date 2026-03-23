"""
TizenClaw Voice Channel — STT/TTS integration via ctypes FFI.

Binds to Tizen native libstt.so and libtts.so shared libraries.
Provides:
  - STT (Speech-To-Text): record → recognize → text
  - TTS (Text-To-Speech): text → synthesize → play
  - Voice interaction loop: listen → AgentCore → speak response

All native API calls are made through ctypes with proper
callback function types (CFUNCTYPE).
"""
import asyncio
import ctypes
import ctypes.util
import logging
import os
import threading
import time
from enum import IntEnum
from typing import Optional, Callable, Dict, Any

logger = logging.getLogger(__name__)

# ── Tizen STT/TTS Error Codes ──

STT_ERROR_NONE = 0
TTS_ERROR_NONE = 0

# ── STT States ──

class SttState(IntEnum):
    CREATED = 0
    READY = 1
    RECORDING = 2
    PROCESSING = 3

# ── TTS States ──

class TtsState(IntEnum):
    CREATED = 0
    READY = 1
    PLAYING = 2
    PAUSED = 3

# ── STT Recognition Result Events ──

class SttResultEvent(IntEnum):
    PARTIAL = 0
    FINAL = 1

# ── Callback function prototypes (C types) ──

# stt_recognition_result_cb: void (*)(stt_h, stt_result_event_e, const char**, int, const char*, void*)
STT_RECOGNITION_RESULT_CB = ctypes.CFUNCTYPE(
    None,
    ctypes.c_void_p,    # stt_h
    ctypes.c_int,       # event (SttResultEvent)
    ctypes.POINTER(ctypes.c_char_p),  # data (char**)
    ctypes.c_int,       # data_count
    ctypes.c_char_p,    # msg
    ctypes.c_void_p,    # user_data
)

# stt_state_changed_cb: void (*)(stt_h, stt_state_e, stt_state_e, void*)
STT_STATE_CHANGED_CB = ctypes.CFUNCTYPE(
    None,
    ctypes.c_void_p,    # stt_h
    ctypes.c_int,       # previous state
    ctypes.c_int,       # current state
    ctypes.c_void_p,    # user_data
)

# tts_state_changed_cb: void (*)(tts_h, tts_state_e, tts_state_e, void*)
TTS_STATE_CHANGED_CB = ctypes.CFUNCTYPE(
    None,
    ctypes.c_void_p,    # tts_h
    ctypes.c_int,       # previous state
    ctypes.c_int,       # current state
    ctypes.c_void_p,    # user_data
)

# tts_utterance_completed_cb: void (*)(tts_h, int, void*)
TTS_UTTERANCE_COMPLETED_CB = ctypes.CFUNCTYPE(
    None,
    ctypes.c_void_p,    # tts_h
    ctypes.c_int,       # utt_id
    ctypes.c_void_p,    # user_data
)


class SttEngine:
    """Tizen STT (Speech-To-Text) engine wrapper via ctypes."""

    def __init__(self):
        self._lib = None
        self._handle = ctypes.c_void_p(0)
        self._state = SttState.CREATED
        self._result_text = ""
        self._result_event = asyncio.Event()
        self._ready_event = asyncio.Event()
        self._initialized = False
        # Must keep references to prevent GC of callbacks
        self._recognition_cb_ref = None
        self._state_cb_ref = None

    def _load_lib(self) -> bool:
        try:
            self._lib = ctypes.CDLL("libstt.so")
            logger.info("SttEngine: libstt.so loaded")
            return True
        except OSError as e:
            logger.error(f"SttEngine: Failed to load libstt.so: {e}")
            return False

    def initialize(self) -> bool:
        """Create and prepare STT handle."""
        if not self._load_lib():
            return False

        # stt_create(stt_h *stt) -> int
        handle = ctypes.c_void_p(0)
        ret = self._lib.stt_create(ctypes.byref(handle))
        if ret != STT_ERROR_NONE:
            logger.error(f"SttEngine: stt_create failed: {ret}")
            return False
        self._handle = handle

        # Set state changed callback
        self._state_cb_ref = STT_STATE_CHANGED_CB(self._on_state_changed)
        ret = self._lib.stt_set_state_changed_cb(
            self._handle, self._state_cb_ref, None
        )
        if ret != STT_ERROR_NONE:
            logger.warning(f"SttEngine: set_state_changed_cb failed: {ret}")

        # Set recognition result callback
        self._recognition_cb_ref = STT_RECOGNITION_RESULT_CB(self._on_recognition_result)
        ret = self._lib.stt_set_recognition_result_cb(
            self._handle, self._recognition_cb_ref, None
        )
        if ret != STT_ERROR_NONE:
            logger.warning(f"SttEngine: set_recognition_result_cb failed: {ret}")

        # Prepare (connects to STT server)
        ret = self._lib.stt_prepare(self._handle)
        if ret != STT_ERROR_NONE:
            logger.error(f"SttEngine: stt_prepare failed: {ret}")
            return False

        self._initialized = True
        logger.info("SttEngine: Initialized and preparing...")
        return True

    def shutdown(self):
        """Destroy STT handle."""
        if self._lib and self._handle:
            try:
                self._lib.stt_unprepare(self._handle)
            except Exception:
                pass
            self._lib.stt_destroy(self._handle)
            self._handle = ctypes.c_void_p(0)
            logger.info("SttEngine: Destroyed")
        self._initialized = False

    def _on_state_changed(self, stt_h, prev, curr, user_data):
        """Native callback: STT state changed."""
        self._state = SttState(curr)
        logger.debug(f"SttEngine: State {SttState(prev).name} → {SttState(curr).name}")
        if curr == SttState.READY:
            self._ready_event.set()

    def _on_recognition_result(self, stt_h, event, data, data_count, msg, user_data):
        """Native callback: recognition result received."""
        if data_count > 0 and data:
            text_parts = []
            for i in range(data_count):
                if data[i]:
                    text_parts.append(data[i].decode("utf-8", errors="replace"))
            self._result_text = " ".join(text_parts)
        elif msg:
            self._result_text = msg.decode("utf-8", errors="replace")

        if event == SttResultEvent.FINAL:
            logger.info(f"SttEngine: Recognized: {self._result_text[:80]}")
            self._result_event.set()

    async def wait_ready(self, timeout: float = 10.0) -> bool:
        """Wait for STT engine to be ready."""
        try:
            await asyncio.wait_for(self._ready_event.wait(), timeout)
            return True
        except asyncio.TimeoutError:
            return False

    def start_recording(self, language: str = "ko_KR",
                        rec_type: str = "default") -> bool:
        """Start speech recognition."""
        if not self._initialized:
            return False

        self._result_text = ""
        self._result_event.clear()

        ret = self._lib.stt_start(
            self._handle,
            language.encode("utf-8"),
            rec_type.encode("utf-8"),
        )
        if ret != STT_ERROR_NONE:
            logger.error(f"SttEngine: stt_start failed: {ret}")
            return False

        logger.info("SttEngine: Recording started")
        return True

    def stop_recording(self) -> bool:
        """Stop speech recognition."""
        if not self._initialized:
            return False

        ret = self._lib.stt_stop(self._handle)
        if ret != STT_ERROR_NONE:
            logger.error(f"SttEngine: stt_stop failed: {ret}")
            return False

        logger.info("SttEngine: Recording stopped, processing...")
        return True

    async def recognize(self, language: str = "ko_KR",
                        listen_seconds: float = 5.0,
                        timeout: float = 15.0) -> str:
        """Full recognition flow: start → wait → stop → result."""
        if not self.start_recording(language):
            return ""

        await asyncio.sleep(listen_seconds)
        self.stop_recording()

        try:
            await asyncio.wait_for(self._result_event.wait(), timeout)
        except asyncio.TimeoutError:
            logger.warning("SttEngine: Recognition timeout")

        return self._result_text

    def get_state(self) -> str:
        return self._state.name


class TtsEngine:
    """Tizen TTS (Text-To-Speech) engine wrapper via ctypes."""

    def __init__(self):
        self._lib = None
        self._handle = ctypes.c_void_p(0)
        self._state = TtsState.CREATED
        self._utterance_done = asyncio.Event()
        self._ready_event = asyncio.Event()
        self._initialized = False
        self._state_cb_ref = None
        self._utterance_cb_ref = None

    def _load_lib(self) -> bool:
        try:
            self._lib = ctypes.CDLL("libtts.so")
            logger.info("TtsEngine: libtts.so loaded")
            return True
        except OSError as e:
            logger.error(f"TtsEngine: Failed to load libtts.so: {e}")
            return False

    def initialize(self) -> bool:
        """Create and prepare TTS handle."""
        if not self._load_lib():
            return False

        handle = ctypes.c_void_p(0)
        ret = self._lib.tts_create(ctypes.byref(handle))
        if ret != TTS_ERROR_NONE:
            logger.error(f"TtsEngine: tts_create failed: {ret}")
            return False
        self._handle = handle

        # Set state changed callback
        self._state_cb_ref = TTS_STATE_CHANGED_CB(self._on_state_changed)
        ret = self._lib.tts_set_state_changed_cb(
            self._handle, self._state_cb_ref, None
        )

        # Set utterance completed callback
        self._utterance_cb_ref = TTS_UTTERANCE_COMPLETED_CB(self._on_utterance_completed)
        ret = self._lib.tts_set_utterance_completed_cb(
            self._handle, self._utterance_cb_ref, None
        )

        # Prepare
        ret = self._lib.tts_prepare(self._handle)
        if ret != TTS_ERROR_NONE:
            logger.error(f"TtsEngine: tts_prepare failed: {ret}")
            return False

        self._initialized = True
        logger.info("TtsEngine: Initialized and preparing...")
        return True

    def shutdown(self):
        if self._lib and self._handle:
            try:
                self._lib.tts_unprepare(self._handle)
            except Exception:
                pass
            self._lib.tts_destroy(self._handle)
            self._handle = ctypes.c_void_p(0)
            logger.info("TtsEngine: Destroyed")
        self._initialized = False

    def _on_state_changed(self, tts_h, prev, curr, user_data):
        self._state = TtsState(curr)
        logger.debug(f"TtsEngine: State {TtsState(prev).name} → {TtsState(curr).name}")
        if curr == TtsState.READY:
            self._ready_event.set()

    def _on_utterance_completed(self, tts_h, utt_id, user_data):
        logger.debug(f"TtsEngine: Utterance {utt_id} completed")
        self._utterance_done.set()

    async def wait_ready(self, timeout: float = 10.0) -> bool:
        try:
            await asyncio.wait_for(self._ready_event.wait(), timeout)
            return True
        except asyncio.TimeoutError:
            return False

    async def speak(self, text: str, language: str = "ko_KR",
                    voice_type: int = 1,  # 1 = female
                    speed: int = 0,  # 0 = auto
                    timeout: float = 30.0) -> bool:
        """Speak text using TTS. Returns True on completion."""
        if not self._initialized:
            return False

        self._utterance_done.clear()

        # tts_add_text(tts_h, text, language, voice_type, speed, &utt_id)
        utt_id = ctypes.c_int(0)
        ret = self._lib.tts_add_text(
            self._handle,
            text.encode("utf-8"),
            language.encode("utf-8") if language else None,
            ctypes.c_int(voice_type),
            ctypes.c_int(speed),
            ctypes.byref(utt_id),
        )
        if ret != TTS_ERROR_NONE:
            logger.error(f"TtsEngine: tts_add_text failed: {ret}")
            return False

        # Play
        ret = self._lib.tts_play(self._handle)
        if ret != TTS_ERROR_NONE:
            logger.error(f"TtsEngine: tts_play failed: {ret}")
            return False

        logger.info(f"TtsEngine: Speaking: {text[:50]}...")

        # Wait for utterance completion
        try:
            await asyncio.wait_for(self._utterance_done.wait(), timeout)
            return True
        except asyncio.TimeoutError:
            logger.warning("TtsEngine: Speak timeout")
            return False

    def stop(self) -> bool:
        if self._initialized and self._lib:
            ret = self._lib.tts_stop(self._handle)
            return ret == TTS_ERROR_NONE
        return False

    def get_state(self) -> str:
        return self._state.name


class VoiceChannel:
    """
    Voice interaction channel combining STT + TTS.
    Provides a listen → process → speak loop.
    """

    def __init__(self):
        self.stt = SttEngine()
        self.tts = TtsEngine()
        self._agent = None
        self._running = False
        self._task: Optional[asyncio.Task] = None
        self._language = "ko_KR"
        self._listen_seconds = 5.0
        self._enabled = False

    async def start(self, agent_core, language: str = "ko_KR") -> bool:
        """Initialize STT/TTS engines and start voice loop."""
        self._agent = agent_core
        self._language = language

        # Initialize STT
        stt_ok = self.stt.initialize()
        if stt_ok:
            await self.stt.wait_ready(timeout=5.0)
            logger.info("VoiceChannel: STT ready")
        else:
            logger.warning("VoiceChannel: STT not available")

        # Initialize TTS
        tts_ok = self.tts.initialize()
        if tts_ok:
            await self.tts.wait_ready(timeout=5.0)
            logger.info("VoiceChannel: TTS ready")
        else:
            logger.warning("VoiceChannel: TTS not available")

        self._enabled = stt_ok and tts_ok
        if self._enabled:
            logger.info(f"VoiceChannel: Ready (lang={language})")
        else:
            logger.warning("VoiceChannel: Partially available")

        return stt_ok or tts_ok

    async def stop(self):
        self._running = False
        if self._task:
            self._task.cancel()
            try:
                await self._task
            except asyncio.CancelledError:
                pass
        self.stt.shutdown()
        self.tts.shutdown()
        logger.info("VoiceChannel: Stopped")

    async def start_voice_loop(self):
        """Start continuous listen → respond → speak loop."""
        self._running = True
        self._task = asyncio.create_task(self._voice_loop())

    async def _voice_loop(self):
        """Main voice interaction loop."""
        while self._running:
            try:
                # Listen
                text = await self.stt.recognize(
                    language=self._language,
                    listen_seconds=self._listen_seconds,
                )
                if not text or not text.strip():
                    await asyncio.sleep(0.5)
                    continue

                logger.info(f"VoiceChannel: Heard: {text}")

                # Process through AgentCore
                if self._agent:
                    response = await self._agent.process_prompt("voice", text)

                    # Speak response
                    if response:
                        await self.tts.speak(response, language=self._language)

            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"VoiceChannel: Loop error: {e}")
                await asyncio.sleep(2)

    async def listen_once(self) -> str:
        """Listen for a single utterance and return text."""
        return await self.stt.recognize(
            language=self._language,
            listen_seconds=self._listen_seconds,
        )

    async def speak(self, text: str) -> bool:
        """Speak text using TTS."""
        return await self.tts.speak(text, language=self._language)

    def get_status(self) -> Dict[str, Any]:
        return {
            "enabled": self._enabled,
            "running": self._running,
            "language": self._language,
            "stt_state": self.stt.get_state(),
            "tts_state": self.tts.get_state(),
        }
