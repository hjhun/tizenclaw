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
#ifndef VOICE_CHANNEL_HH
#define VOICE_CHANNEL_HH

#include <atomic>
#include <string>
#include <thread>

#include "channel.hh"

// Conditionally include Tizen STT/TTS headers
#ifdef TIZEN_STT_ENABLED
#include <stt.h>
#endif
#ifdef TIZEN_TTS_ENABLED
#include <tts.h>
#endif

namespace tizenclaw {

class AgentCore;

// Voice control channel using Tizen native
// STT (Speech-to-Text) and TTS
// (Text-to-Speech) C-API.
// This channel is conditionally compiled:
// if Tizen STT/TTS packages are not available,
// it compiles as a stub that logs a warning.
class VoiceChannel : public Channel {
 public:
  explicit VoiceChannel(AgentCore* agent);
  ~VoiceChannel();

  // Channel interface
  std::string GetName() const override { return "voice"; }
  bool Start() override;
  void Stop() override;
  bool IsRunning() const override { return running_; }
  bool SendMessage(const std::string& text) override;

 private:
#ifdef TIZEN_STT_ENABLED
  // STT callbacks
  static void OnSttResult(stt_h stt, stt_result_event_e event,
                          const char** data, int data_count, const char* msg,
                          void* user_data);
  static void OnSttState(stt_h stt, stt_state_e previous, stt_state_e current,
                         void* user_data);

  bool InitStt();
  void StartListening();
  void StopListening();
  stt_h stt_ = nullptr;
#endif

#ifdef TIZEN_TTS_ENABLED
  // TTS callbacks
  static void OnTtsUtterance(tts_h tts, int utt_id,
                             tts_utterance_status_e status, void* user_data);
  static void OnTtsState(tts_h tts, tts_state_e previous, tts_state_e current,
                         void* user_data);

  bool InitTts();
  void Speak(const std::string& text);
  tts_h tts_ = nullptr;
#endif

  void ProcessVoiceInput(const std::string& text);

  AgentCore* agent_;
  std::atomic<bool> running_{false};
  std::string session_id_ = "voice_default";
};

}  // namespace tizenclaw

#endif  // VOICE_CHANNEL_HH
