/*
 * Copyright (c) 2026 Samsung Electronics Co., Ltd All Rights Reserved
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
#include "voice_channel.hh"

#include "../../common/logging.hh"
#include "../core/agent_core.hh"

namespace tizenclaw {

VoiceChannel::VoiceChannel(AgentCore* agent) : agent_(agent) {}

VoiceChannel::~VoiceChannel() { Stop(); }

void VoiceChannel::ProcessVoiceInput(const std::string& text) {
  if (!agent_ || text.empty()) return;

  LOG(INFO) << "Voice input: " << text;

  std::string response = agent_->ProcessPrompt(session_id_, text);

  LOG(INFO) << "Voice response: "
            << response.substr(0, std::min((size_t)100, response.size()));

#ifdef TIZEN_TTS_ENABLED
  Speak(response);
#else
  LOG(INFO) << "TTS not available — response " << "text only";
#endif
}

// ============================================
// STT Implementation (conditional)
// ============================================
#ifdef TIZEN_STT_ENABLED

bool VoiceChannel::InitStt() {
  int ret = stt_create(&stt_);
  if (ret != STT_ERROR_NONE) {
    LOG(ERROR) << "stt_create failed: " << ret;
    return false;
  }

  ret = stt_set_recognition_result_cb(stt_, OnSttResult, this);
  if (ret != STT_ERROR_NONE) {
    LOG(ERROR) << "stt_set_result_cb failed: " << ret;
    stt_destroy(stt_);
    stt_ = nullptr;
    return false;
  }

  ret = stt_set_state_changed_cb(stt_, OnSttState, this);
  if (ret != STT_ERROR_NONE) {
    LOG(ERROR) << "stt_set_state_cb failed: " << ret;
    stt_destroy(stt_);
    stt_ = nullptr;
    return false;
  }

  ret = stt_prepare(stt_);
  if (ret != STT_ERROR_NONE) {
    LOG(ERROR) << "stt_prepare failed: " << ret;
    stt_destroy(stt_);
    stt_ = nullptr;
    return false;
  }

  LOG(INFO) << "STT initialized successfully";
  return true;
}

void VoiceChannel::StartListening() {
  if (!stt_) return;

  int ret = stt_start(stt_, nullptr, STT_RECOGNITION_TYPE_FREE);
  if (ret != STT_ERROR_NONE) {
    LOG(ERROR) << "stt_start failed: " << ret;
  } else {
    LOG(INFO) << "STT listening started";
  }
}

void VoiceChannel::StopListening() {
  if (!stt_) return;

  stt_state_e state;
  stt_get_state(stt_, &state);
  if (state == STT_STATE_RECORDING) {
    stt_stop(stt_);
  }
}

void VoiceChannel::OnSttResult(stt_h /*stt*/, stt_result_event_e event,
                               const char** data, int data_count,
                               const char* /*msg*/, void* user_data) {
  auto* self = static_cast<VoiceChannel*>(user_data);

  if (event == STT_RESULT_EVENT_FINAL_RESULT && data_count > 0 && data[0]) {
    std::string text(data[0]);
    LOG(INFO) << "STT result: " << text;
    self->ProcessVoiceInput(text);

    // Continue listening
    if (self->running_) {
      self->StartListening();
    }
  }
}

void VoiceChannel::OnSttState(stt_h /*stt*/, stt_state_e /*previous*/,
                              stt_state_e current, void* /*user_data*/) {
  LOG(INFO) << "STT state changed to: " << static_cast<int>(current);
}

#endif  // TIZEN_STT_ENABLED

// ============================================
// TTS Implementation (conditional)
// ============================================
#ifdef TIZEN_TTS_ENABLED

bool VoiceChannel::InitTts() {
  int ret = tts_create(&tts_);
  if (ret != TTS_ERROR_NONE) {
    LOG(ERROR) << "tts_create failed: " << ret;
    return false;
  }

  ret = tts_set_utterance_completed_cb(tts_, OnTtsUtterance, this);
  if (ret != TTS_ERROR_NONE) {
    LOG(WARNING) << "tts_set_utterance_cb failed: " << ret;
  }

  ret = tts_set_state_changed_cb(tts_, OnTtsState, this);
  if (ret != TTS_ERROR_NONE) {
    LOG(WARNING) << "tts_set_state_cb failed: " << ret;
  }

  ret = tts_prepare(tts_);
  if (ret != TTS_ERROR_NONE) {
    LOG(ERROR) << "tts_prepare failed: " << ret;
    tts_destroy(tts_);
    tts_ = nullptr;
    return false;
  }

  LOG(INFO) << "TTS initialized successfully";
  return true;
}

void VoiceChannel::Speak(const std::string& text) {
  if (!tts_ || text.empty()) return;

  int utt_id = 0;
  int ret = tts_add_text(tts_, text.c_str(), nullptr, TTS_VOICE_TYPE_AUTO,
                         TTS_SPEED_AUTO, &utt_id);
  if (ret != TTS_ERROR_NONE) {
    LOG(ERROR) << "tts_add_text failed: " << ret;
    return;
  }

  ret = tts_play(tts_);
  if (ret != TTS_ERROR_NONE) {
    LOG(ERROR) << "tts_play failed: " << ret;
  }
}

void VoiceChannel::OnTtsUtterance(tts_h /*tts*/, int utt_id,
                                  tts_utterance_status_e status,
                                  void* /*user_data*/) {
  LOG(INFO) << "TTS utterance " << utt_id
            << " status: " << static_cast<int>(status);
}

void VoiceChannel::OnTtsState(tts_h /*tts*/, tts_state_e /*previous*/,
                              tts_state_e current, void* /*user_data*/) {
  LOG(INFO) << "TTS state changed to: " << static_cast<int>(current);
}

#endif  // TIZEN_TTS_ENABLED

// ============================================
// Channel interface
// ============================================

bool VoiceChannel::Start() {
  if (running_) return true;

#if !defined(TIZEN_STT_ENABLED) && !defined(TIZEN_TTS_ENABLED)
  LOG(WARNING) << "Voice channel: STT/TTS " << "not available in this build";
  return false;
#endif

#ifdef TIZEN_STT_ENABLED
  if (!InitStt()) {
    LOG(ERROR) << "Voice channel: STT init " << "failed";
    return false;
  }
#endif

#ifdef TIZEN_TTS_ENABLED
  if (!InitTts()) {
    LOG(WARNING) << "Voice channel: TTS " << "init failed (non-fatal)";
  }
#endif

  running_ = true;

#ifdef TIZEN_STT_ENABLED
  StartListening();
#endif

  LOG(INFO) << "VoiceChannel started";
  return true;
}

void VoiceChannel::Stop() {
  if (!running_) return;

  running_ = false;

#ifdef TIZEN_STT_ENABLED
  StopListening();
  if (stt_) {
    stt_unprepare(stt_);
    stt_destroy(stt_);
    stt_ = nullptr;
  }
#endif

#ifdef TIZEN_TTS_ENABLED
  if (tts_) {
    tts_stop(tts_);
    tts_unprepare(tts_);
    tts_destroy(tts_);
    tts_ = nullptr;
  }
#endif

  LOG(INFO) << "VoiceChannel stopped";
}

bool VoiceChannel::SendMessage(
    const std::string& text) {
  if (!running_ || text.empty()) return false;
#ifdef TIZEN_TTS_ENABLED
  Speak(text);
  return true;
#else
  LOG(WARNING) << "Voice SendMessage: TTS "
               << "not available";
  return false;
#endif
}

}  // namespace tizenclaw
