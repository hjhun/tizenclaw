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
#ifndef SCREEN_PERCEPTOR_HH
#define SCREEN_PERCEPTOR_HH

#include <atomic>
#include <chrono>
#include <cstdint>
#include <functional>
#include <json.hpp>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
#include <unordered_map>
#include <vector>

#include <screen_connector_toolkit.h>

namespace tizenclaw {

// Represents metadata for a single visible window surface
struct SurfaceInfo {
  std::string appid;
  std::string instance_id;
  int pid = 0;
  bool visible = true;
  std::chrono::steady_clock::time_point last_update;
};

// Represents a captured frame from a visible surface
struct CapturedFrame {
  std::string appid;
  std::string instance_id;
  int width = 0;
  int height = 0;
  uint32_t format = 0;       // TBM format (e.g. ARGB8888)
  std::vector<uint8_t> data;  // Raw pixel data copy
  std::chrono::steady_clock::time_point timestamp;
};

// Callback type for when a new frame is captured
using FrameCapturedCallback =
    std::function<void(const CapturedFrame& frame)>;

// ScreenPerceptor: Captures visible app window buffers
// via Tizen's screen-connector (Wayland remote surface)
// API at low frequency (1-2 frames per 3 seconds) for
// agent visual context awareness.
//
// Architecture:
//   screen-connector (SCREEN_TYPE_ALL)
//       → filter visible windows only
//       → periodic buffer sampling (3s interval)
//       → copy raw pixels
//       → invoke callback for Vision/OCR pipeline
//
// This class relies on ecore_wl2 and screen_connector
// libraries at compile-time.
class ScreenPerceptor {
 public:
  ScreenPerceptor();
  ~ScreenPerceptor();

  // Non-copyable, non-movable
  ScreenPerceptor(const ScreenPerceptor&) = delete;
  ScreenPerceptor& operator=(const ScreenPerceptor&) = delete;

  // Initialize and start screen perception
  // Returns false if screen-connector is unavailable
  bool Start();

  // Stop screen perception and clean up
  void Stop();

  // Check if currently running
  [[nodiscard]] bool IsRunning() const {
    return running_.load();
  }

  // Register a callback for captured frames
  void SetFrameCallback(FrameCapturedCallback cb);

  // Get list of currently tracked surfaces
  [[nodiscard]] nlohmann::json GetTrackedSurfaces() const;

  // Get the latest captured frame info (metadata only)
  [[nodiscard]] nlohmann::json GetStatus() const;

  // Get the latest captured text context (from OCR)
  [[nodiscard]] std::string GetLatestScreenContext() const;

  // Set the screen context text (called by OCR pipeline)
  void SetScreenContext(const std::string& context);

 private:
  // Dynamic loading of screen-connector library
  bool LoadLibrary();
  void UnloadLibrary();

  // screen-connector callbacks (static, forwarded to instance)
  static void OnSurfaceAdded(const char* appid,
                             const char* instance_id,
                             const int pid, void* data);
  static void OnSurfaceRemoved(const char* appid,
                               const char* instance_id,
                               const int pid, void* data);
  static void OnSurfaceUpdated(
      struct tizen_remote_surface* trs,
      uint32_t type,
      struct wl_buffer* tbm,
      int32_t img_file_fd,
      uint32_t img_file_size,
      uint32_t time,
      struct wl_array* keys,
      const char* appid,
      const char* instance_id,
      const int pid, void* data);

  // Instance-level handlers
  void HandleSurfaceAdded(const std::string& appid,
                          const std::string& instance_id,
                          int pid);
  void HandleSurfaceRemoved(const std::string& appid,
                            const std::string& instance_id,
                            int pid);
  void HandleSurfaceUpdated(const std::string& appid,
                            const std::string& instance_id,
                            int pid,
                            struct wl_buffer* tbm);

  // Periodic sampling thread
  void SamplingLoop();

  // Check if enough time has elapsed since last capture
  bool ShouldCapture() const;

  // Extract pixel data from wl_buffer/TBM surface
  bool ExtractPixels(struct wl_buffer* tbm,
                     CapturedFrame& frame);

  void EcoreLoop();

  screen_connector_toolkit_h toolkit_handle_ = nullptr;
  void* wl2_display_ = nullptr;

  // Tracked surfaces
  mutable std::mutex surfaces_mutex_;
  std::unordered_map<std::string, SurfaceInfo> surfaces_;

  // Capture state
  std::atomic<bool> running_{false};
  std::thread sampling_thread_;
  std::thread ecore_thread_;
  FrameCapturedCallback frame_callback_;

  // Latest screen context (OCR result)
  mutable std::mutex context_mutex_;
  std::string latest_screen_context_;

  // Last capture timestamp for rate-limiting
  std::chrono::steady_clock::time_point last_capture_time_;

  // Sampling interval: 3 seconds
  static constexpr int kSamplingIntervalMs = 3000;

  // Maximum frames to process per sampling tick
  static constexpr int kMaxFramesPerTick = 2;
};

}  // namespace tizenclaw

#endif  // SCREEN_PERCEPTOR_HH
