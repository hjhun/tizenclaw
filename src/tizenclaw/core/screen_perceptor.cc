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

#include <dlfcn.h>
#include <dlog.h>
#include <tbm_surface.h>

#include <algorithm>
#include <cstring>

#ifdef LOG_TAG
#undef LOG_TAG
#endif
#define LOG_TAG "TIZENCLAW_SCREEN"

// AUL screen type enum value for ALL
// (matches AUL_SCREEN_TYPE_ALL from aul_screen_connector.h)
static constexpr int kScreenTypeAll = 0xFFFF;

namespace tizenclaw {

ScreenPerceptor::ScreenPerceptor() = default;

ScreenPerceptor::~ScreenPerceptor() {
  Stop();
}

bool ScreenPerceptor::LoadLibrary() {
  lib_handle_ = dlopen(kLibScreenConnector, RTLD_LAZY);
  if (!lib_handle_) {
    LOGW("screen-connector library not available: %s",
         dlerror());
    return false;
  }

  fn_init_ = reinterpret_cast<InitFn>(
      dlsym(lib_handle_, "screen_connector_toolkit_init"));
  fn_fini_ = reinterpret_cast<FiniFn>(
      dlsym(lib_handle_, "screen_connector_toolkit_fini"));
  fn_add_ = reinterpret_cast<AddFn>(
      dlsym(lib_handle_, "screen_connector_toolkit_add"));
  fn_remove_ = reinterpret_cast<RemoveFn>(
      dlsym(lib_handle_,
            "screen_connector_toolkit_remove"));

  if (!fn_init_ || !fn_fini_ || !fn_add_ || !fn_remove_) {
    LOGE("Failed to resolve screen-connector symbols");
    UnloadLibrary();
    return false;
  }

  LOGI("screen-connector library loaded successfully");
  return true;
}

void ScreenPerceptor::UnloadLibrary() {
  if (lib_handle_) {
    dlclose(lib_handle_);
    lib_handle_ = nullptr;
  }
  fn_init_ = nullptr;
  fn_fini_ = nullptr;
  fn_add_ = nullptr;
  fn_remove_ = nullptr;
}

bool ScreenPerceptor::Start() {
  if (running_.load()) {
    LOGW("ScreenPerceptor already running");
    return true;
  }

  if (!LoadLibrary()) {
    LOGW("ScreenPerceptor: screen-connector unavailable, "
         "visual perception disabled");
    return false;
  }

  // Initialize screen-connector for ALL screen types
  int ret = fn_init_(kScreenTypeAll);
  if (ret != 0) {
    LOGE("screen_connector_toolkit_init(ALL) failed: %d",
         ret);
    UnloadLibrary();
    return false;
  }

  // Set up callbacks structure
  // We use a static struct that lives for the duration of
  // the ScreenPerceptor — the ops struct is copied by the
  // toolkit internally.
  struct ToolkitOps {
    void (*added_cb)(const char*, const char*, int, void*);
    void (*removed_cb)(const char*, const char*, int, void*);
    void (*updated_cb)(void*, uint32_t, void*, int32_t,
                       uint32_t, uint32_t, void*,
                       const char*, const char*, int,
                       void*);
  };

  static ToolkitOps ops{};
  ops.added_cb = &ScreenPerceptor::OnSurfaceAdded;
  ops.removed_cb = &ScreenPerceptor::OnSurfaceRemoved;
  ops.updated_cb = reinterpret_cast<decltype(ops.updated_cb)>(
      &ScreenPerceptor::OnSurfaceUpdated);

  // Register toolkit with ALL type, passing 'this' as
  // user data
  toolkit_handle_ = fn_add_(
      reinterpret_cast<void*>(&ops),
      "org.tizen.tizenclaw",  // id: valid string instead of "" or nullptr
      kScreenTypeAll,
      this);

  if (!toolkit_handle_) {
    LOGE("screen_connector_toolkit_add failed");
    fn_fini_(kScreenTypeAll);
    UnloadLibrary();
    return false;
  }

  running_.store(true);
  last_capture_time_ = std::chrono::steady_clock::now();

  LOGI("ScreenPerceptor started — watching all visible "
       "surfaces (sampling every %dms)",
       kSamplingIntervalMs);
  return true;
}

void ScreenPerceptor::Stop() {
  if (!running_.load()) return;

  running_.store(false);

  if (sampling_thread_.joinable()) {
    sampling_thread_.join();
  }

  if (toolkit_handle_ && fn_remove_) {
    fn_remove_(toolkit_handle_);
    toolkit_handle_ = nullptr;
  }

  if (fn_fini_) {
    fn_fini_(kScreenTypeAll);
  }

  UnloadLibrary();

  std::lock_guard<std::mutex> lock(surfaces_mutex_);
  surfaces_.clear();

  LOGI("ScreenPerceptor stopped");
}

void ScreenPerceptor::SetFrameCallback(
    FrameCapturedCallback cb) {
  frame_callback_ = std::move(cb);
}

// Static callbacks — forward to instance via user_data
void ScreenPerceptor::OnSurfaceAdded(
    const char* appid, const char* instance_id,
    int pid, void* data) {
  auto* self = static_cast<ScreenPerceptor*>(data);
  if (!self || !self->running_.load()) return;

  std::string app = appid ? appid : "";
  std::string inst = instance_id ? instance_id : "";
  self->HandleSurfaceAdded(app, inst, pid);
}

void ScreenPerceptor::OnSurfaceRemoved(
    const char* appid, const char* instance_id,
    int pid, void* data) {
  auto* self = static_cast<ScreenPerceptor*>(data);
  if (!self || !self->running_.load()) return;

  std::string app = appid ? appid : "";
  std::string inst = instance_id ? instance_id : "";
  self->HandleSurfaceRemoved(app, inst, pid);
}

void ScreenPerceptor::OnSurfaceUpdated(
    struct tizen_remote_surface* /*trs*/,
    uint32_t /*type*/,
    struct wl_buffer* tbm,
    int32_t /*img_file_fd*/,
    uint32_t /*img_file_size*/,
    uint32_t /*time*/,
    struct wl_array* /*keys*/,
    const char* appid,
    const char* instance_id,
    int pid, void* data) {
  auto* self = static_cast<ScreenPerceptor*>(data);
  if (!self || !self->running_.load()) return;

  std::string app = appid ? appid : "";
  std::string inst = instance_id ? instance_id : "";
  self->HandleSurfaceUpdated(app, inst, pid, tbm);
}

// Instance handlers
void ScreenPerceptor::HandleSurfaceAdded(
    const std::string& appid,
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

  LOGI("Surface added: %s (PID %d)", appid.c_str(), pid);
}

void ScreenPerceptor::HandleSurfaceRemoved(
    const std::string& appid,
    const std::string& instance_id,
    int /*pid*/) {
  std::lock_guard<std::mutex> lock(surfaces_mutex_);

  std::string key = appid + ":" + instance_id;
  surfaces_.erase(key);

  LOGI("Surface removed: %s", appid.c_str());
}

void ScreenPerceptor::HandleSurfaceUpdated(
    const std::string& appid,
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

    LOGI("Frame captured from %s (%dx%d, %zu bytes)",
         appid.c_str(), frame.width, frame.height,
         frame.data.size());

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
    LOGE("Failed to map tbm_surface_h for pixel extraction");
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
    LOGW("TBM surface mapped, but no planes available");
  }

  tbm_surface_unmap(surface);

  LOGD("ExtractPixels Success: %dx%d, format=%x, bytes=%zu",
       frame.width, frame.height, frame.format, frame.data.size());

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
  status["library_loaded"] = (lib_handle_ != nullptr);

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
