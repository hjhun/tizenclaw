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
#include "screen_perceptor.hh"

#include <algorithm>
#include <cstring>
#include <cstdlib>

#include "../../common/logging.hh"

#include <Ecore.h>
#include <Ecore_Wl2.h>
#include <screen_connector_toolkit.h>
#include <tbm_surface.h>

namespace tizenclaw {

ScreenPerceptor::ScreenPerceptor() = default;

ScreenPerceptor::~ScreenPerceptor() {
  Stop();
}

bool ScreenPerceptor::LoadLibrary() {
  return true;
}

void ScreenPerceptor::UnloadLibrary() {}

bool ScreenPerceptor::Start() {
  if (running_.load()) return true;



  running_.store(true);
  last_capture_time_ = std::chrono::steady_clock::now();

  LOG(INFO) << "ScreenPerceptor started — watching all visible "
               "surfaces (sampling every " << kSamplingIntervalMs << "ms)";

  ecore_thread_ = std::thread(&ScreenPerceptor::EcoreLoop, this);
  return true;
}

void ScreenPerceptor::Stop() {
  if (!running_.load()) return;

  running_.store(false);

  if (sampling_thread_.joinable()) {
    sampling_thread_.join();
  }

  ecore_main_loop_quit();

  if (ecore_thread_.joinable()) {
    ecore_thread_.join();
  }

  std::lock_guard<std::mutex> lock(surfaces_mutex_);
  surfaces_.clear();

  LOG(INFO) << "ScreenPerceptor stopped";
}

void ScreenPerceptor::SetFrameCallback(
    FrameCapturedCallback cb) {
  frame_callback_ = std::move(cb);
}

// Static callbacks — forward to instance via user_data
void ScreenPerceptor::OnSurfaceAdded(const char* appid,
                                     const char* instance_id,
                                     const int pid, void* data) {
  auto* self = static_cast<ScreenPerceptor*>(data);
  if (!self || !self->running_.load()) return;

  std::string app = appid ? appid : "";
  std::string inst = instance_id ? instance_id : "";
  self->HandleSurfaceAdded(app, inst, pid);
}

void ScreenPerceptor::OnSurfaceRemoved(const char* appid,
                                       const char* instance_id,
                                       const int pid, void* data) {
  auto* self = static_cast<ScreenPerceptor*>(data);
  if (!self || !self->running_.load()) return;

  std::string app = appid ? appid : "";
  std::string inst = instance_id ? instance_id : "";
  self->HandleSurfaceRemoved(app, inst, pid);
}

void ScreenPerceptor::OnSurfaceUpdated(struct tizen_remote_surface* /*trs*/,
                                       uint32_t /*type*/,
                                       struct wl_buffer* tbm,
                                       int32_t /*img_file_fd*/,
                                       uint32_t /*img_file_size*/,
                                       uint32_t /*time*/,
                                       struct wl_array* /*keys*/,
                                       const char* appid,
                                       const char* instance_id,
                                       const int pid, void* data) {
  auto* self = static_cast<ScreenPerceptor*>(data);
  if (!self || !self->running_.load()) return;

  std::string app = appid ? appid : "";
  std::string inst = instance_id ? instance_id : "";
  self->HandleSurfaceUpdated(app, inst, pid, tbm);
}

void ScreenPerceptor::EcoreLoop() {
  // Force XDG_RUNTIME_DIR and WAYLAND_DISPLAY to connect to /run/wayland-0
  setenv("XDG_RUNTIME_DIR", "/run", 1);
  setenv("WAYLAND_DISPLAY", "wayland-0", 1);

  // Initialize Ecore for this thread
  ecore_init();
  ecore_wl2_init();

  wl2_display_ = ecore_wl2_display_connect(nullptr);
  if (!wl2_display_) {
    LOG(ERROR) << "Failed to connect to wayland display via ecore_wl2";
    ecore_wl2_shutdown();
    ecore_shutdown();
    return;
  }

  screen_connector_toolkit_ops ops{};
  ops.added_cb = &ScreenPerceptor::OnSurfaceAdded;
  ops.removed_cb = &ScreenPerceptor::OnSurfaceRemoved;
  ops.updated_cb = &ScreenPerceptor::OnSurfaceUpdated;

  screen_connector_toolkit_init(SCREEN_CONNECTOR_SCREEN_TYPE_ALL);

  toolkit_handle_ = screen_connector_toolkit_add(
      &ops,
      "org.tizen.tizenclaw",
      SCREEN_CONNECTOR_SCREEN_TYPE_ALL,
      this);

  if (!toolkit_handle_) {
    LOG(ERROR) << "Failed to add toolkit in EcoreLoop";
    ecore_wl2_display_disconnect(static_cast<Ecore_Wl2_Display*>(wl2_display_));
    ecore_wl2_shutdown();
    ecore_shutdown();
    return;
  }

  LOG(INFO) << "Starting isolated Ecore loop for Wayland dispatch";

  // This loop block until ecore_main_loop_quit is called
  ecore_main_loop_begin();

  LOG(INFO) << "Isolated Ecore loop finished";

  if (toolkit_handle_) {
    screen_connector_toolkit_remove(toolkit_handle_);
    toolkit_handle_ = nullptr;
  }

  screen_connector_toolkit_fini(SCREEN_CONNECTOR_SCREEN_TYPE_ALL);

  ecore_wl2_display_disconnect(static_cast<Ecore_Wl2_Display*>(wl2_display_));
  wl2_display_ = nullptr;
  ecore_wl2_shutdown();
  ecore_shutdown();
}

// Instance handlers
void ScreenPerceptor::HandleSurfaceAdded(const std::string& appid,
                                         const std::string& instance_id,
                                         int pid) {
  std::lock_guard<std::mutex> lock(surfaces_mutex_);

  std::string key = appid + ":" + instance_id;
  surfaces_[key] = SurfaceInfo{
      .appid = appid,
      .instance_id = instance_id,
      .pid = pid,
      .visible = true,
      .last_update = std::chrono::steady_clock::now()};

  LOG(INFO) << "Surface added: " << appid << " (PID " << pid << ")";
}

void ScreenPerceptor::HandleSurfaceRemoved(const std::string& appid,
                                           const std::string& instance_id,
                                           int /*pid*/) {
  std::lock_guard<std::mutex> lock(surfaces_mutex_);

  std::string key = appid + ":" + instance_id;
  surfaces_.erase(key);

  LOG(INFO) << "Surface removed: " << appid;
}

void ScreenPerceptor::HandleSurfaceUpdated(const std::string& appid,
                                           const std::string& instance_id,
                                           int pid,
                                           struct wl_buffer* tbm) {
  // Rate-limit: only capture if enough time has elapsed
  auto now = std::chrono::steady_clock::now();
  auto elapsed =
      std::chrono::duration_cast<std::chrono::milliseconds>(
          now - last_capture_time_)
          .count();

  if (elapsed < kSamplingIntervalMs) {
    return;  // Skip — too soon since last capture
  }

  // Update surface tracking
  {
    std::lock_guard<std::mutex> lock(surfaces_mutex_);
    std::string key = appid + ":" + instance_id;
    auto it = surfaces_.find(key);
    if (it != surfaces_.end()) {
      it->second.last_update = now;
      it->second.visible = true;
    } else {
      surfaces_[key] = SurfaceInfo{
          .appid = appid,
          .instance_id = instance_id,
          .pid = pid,
          .visible = true,
          .last_update = now};
    }
  }

  // Extract pixels from TBM buffer
  CapturedFrame frame;
  frame.appid = appid;
  frame.instance_id = instance_id;
  frame.timestamp = now;

  if (ExtractPixels(tbm, frame)) {
    last_capture_time_ = now;

    LOG(INFO) << "Frame captured from " << appid << " (" << frame.width << "x" << frame.height << ", " << frame.data.size() << " bytes)";

    // Invoke callback for Vision/OCR pipeline
    if (frame_callback_) {
      frame_callback_(frame);
    }
  }
}

bool ScreenPerceptor::ExtractPixels(
    struct wl_buffer* tbm, CapturedFrame& frame) {
  if (!tbm) return false;

  // In screen-connector context, tbm is typically a tbm_surface_h
  auto surface = reinterpret_cast<tbm_surface_h>(tbm);

  tbm_surface_info_s info;
  if (tbm_surface_map(surface, TBM_SURF_OPTION_READ, &info) !=
      TBM_SURFACE_ERROR_NONE) {
    LOG(ERROR) << "Failed to map tbm_surface_h for pixel extraction";
    return false;
  }

  frame.width = info.width;
  frame.height = info.height;
  frame.format = info.format;

  // Assume ARGB8888 or similar 4-byte packed pixel format for size.
  // In a real implementation, size is computed from info.planes.
  // We'll copy up to the amount of bytes available in the first plane.
  if (info.num_planes > 0 && info.planes[0].ptr != nullptr) {
    size_t size = info.planes[0].stride * info.height;
    frame.data.assign(info.planes[0].ptr,
                      info.planes[0].ptr + size);
  } else {
    LOG(WARNING) << "TBM surface mapped, but no planes available";
  }

  tbm_surface_unmap(surface);

  LOG(INFO) << "ExtractPixels Success: " << frame.width << "x" << frame.height << ", format=" << frame.format << ", bytes=" << frame.data.size();

  return true;
}

bool ScreenPerceptor::ShouldCapture() const {
  auto now = std::chrono::steady_clock::now();
  auto elapsed =
      std::chrono::duration_cast<std::chrono::milliseconds>(
          now - last_capture_time_)
          .count();
  return elapsed >= kSamplingIntervalMs;
}

nlohmann::json ScreenPerceptor::GetTrackedSurfaces() const {
  std::lock_guard<std::mutex> lock(surfaces_mutex_);

  auto arr = nlohmann::json::array();
  for (const auto& [key, info] : surfaces_) {
    arr.push_back({{"appid", info.appid},
                   {"instance_id", info.instance_id},
                   {"pid", info.pid},
                   {"visible", info.visible}});
  }
  return arr;
}

nlohmann::json ScreenPerceptor::GetStatus() const {
  nlohmann::json status;
  status["running"] = running_.load();

  {
    std::lock_guard<std::mutex> lock(surfaces_mutex_);
    status["tracked_surfaces"] = surfaces_.size();
  }

  status["sampling_interval_ms"] = kSamplingIntervalMs;
  status["max_frames_per_tick"] = kMaxFramesPerTick;
  status["library_loaded"] = true;

  {
    std::lock_guard<std::mutex> lock(context_mutex_);
    status["has_screen_context"] =
        !latest_screen_context_.empty();
  }

  return status;
}

std::string ScreenPerceptor::GetLatestScreenContext() const {
  std::lock_guard<std::mutex> lock(context_mutex_);
  return latest_screen_context_;
}

void ScreenPerceptor::SetScreenContext(
    const std::string& context) {
  std::lock_guard<std::mutex> lock(context_mutex_);
  latest_screen_context_ = context;
}

}  // namespace tizenclaw
