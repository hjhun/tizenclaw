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
#include "on_device_embedding.hh"

#include <dlfcn.h>

#include <cmath>
#include <cstring>

#include "../../common/logging.hh"

// Include the official ORT C API header for correct struct layout
#include "onnxruntime_c_api.h"

namespace tizenclaw {

// ─── Globals for dynamically loaded ORT API ─────────────

static const OrtApi* g_ort = nullptr;

#define ORT_CHECK(expr)                                        \
  do {                                                         \
    OrtStatus* _s = (expr);                                    \
    if (_s) {                                                  \
      LOG(ERROR) << #expr << ": "                              \
                 << g_ort->GetErrorMessage(_s);                \
      g_ort->ReleaseStatus(_s);                                \
      return false;                                            \
    }                                                          \
  } while (0)

// ─── OnDeviceEmbedding Implementation ───────────────────

OnDeviceEmbedding::~OnDeviceEmbedding() { Shutdown(); }

bool OnDeviceEmbedding::Initialize(const std::string& model_dir,
                                   const std::string& ort_lib_path) {
  // 1. Load ONNX Runtime via dlopen
  ort_lib_ = dlopen(ort_lib_path.c_str(), RTLD_LAZY);
  if (!ort_lib_) {
    LOG(WARNING) << "ONNX Runtime not found: " << dlerror()
                 << " (on-device embedding disabled)";
    return false;
  }

  // 2. Get OrtGetApiBase
  using GetApiBaseFn = const OrtApiBase* (*)();
  auto get_api_base = reinterpret_cast<GetApiBaseFn>(
      dlsym(ort_lib_, "OrtGetApiBase"));
  if (!get_api_base) {
    LOG(ERROR) << "OrtGetApiBase not found";
    Shutdown();
    return false;
  }

  const OrtApiBase* api_base = get_api_base();
  if (!api_base) {
    LOG(ERROR) << "OrtGetApiBase returned null";
    Shutdown();
    return false;
  }

  // Get API v18 (compatible with v1.20)
  g_ort = api_base->GetApi(ORT_API_VERSION);
  if (!g_ort) {
    LOG(ERROR) << "ORT API not available";
    Shutdown();
    return false;
  }

  LOG(INFO) << "ONNX Runtime loaded: "
            << api_base->GetVersionString();

  // 3. Create environment
  OrtEnv* env = nullptr;
  ORT_CHECK(g_ort->CreateEnv(
      ORT_LOGGING_LEVEL_WARNING, "tizenclaw", &env));
  env_ = env;

  // 4. Create session options
  OrtSessionOptions* opts = nullptr;
  ORT_CHECK(g_ort->CreateSessionOptions(&opts));
  session_options_ = opts;

  // Optimize for inference
  g_ort->SetIntraOpNumThreads(opts, 2);
  g_ort->SetInterOpNumThreads(opts, 1);
  g_ort->SetSessionGraphOptimizationLevel(
      opts, ORT_ENABLE_ALL);
  g_ort->DisableCpuMemArena(opts);
  g_ort->DisableMemPattern(opts);

  // 5. Load model
  std::string model_path = model_dir + "/model.onnx";
  OrtSession* session = nullptr;
  ORT_CHECK(g_ort->CreateSession(
      env, model_path.c_str(), opts, &session));
  session_ = session;

  // 6. Get default allocator
  OrtAllocator* alloc = nullptr;
  ORT_CHECK(g_ort->GetAllocatorWithDefaultOptions(&alloc));
  allocator_ = alloc;

  // 7. Load tokenizer vocabulary
  std::string vocab_path = model_dir + "/vocab.txt";
  if (!tokenizer_.LoadVocab(vocab_path)) {
    LOG(ERROR) << "Failed to load vocab: " << vocab_path;
    Shutdown();
    return false;
  }

  LOG(INFO) << "OnDeviceEmbedding initialized "
            << "(dim=" << kEmbeddingDim << ")";
  return true;
}

void OnDeviceEmbedding::Shutdown() {
  if (g_ort) {
    if (session_) {
      g_ort->ReleaseSession(
          static_cast<OrtSession*>(session_));
      session_ = nullptr;
    }
    if (session_options_) {
      g_ort->ReleaseSessionOptions(
          static_cast<OrtSessionOptions*>(session_options_));
      session_options_ = nullptr;
    }
    if (env_) {
      g_ort->ReleaseEnv(static_cast<OrtEnv*>(env_));
      env_ = nullptr;
    }
  }
  allocator_ = nullptr;
  if (ort_lib_) {
    // Don't unload ORT — some cleanup may still reference it
    // dlclose(ort_lib_);
    ort_lib_ = nullptr;
  }
}

std::vector<float> OnDeviceEmbedding::Encode(
    const std::string& text) {
  if (!session_ || !g_ort || text.empty()) return {};

  // 1. Tokenize
  auto tokens = tokenizer_.Tokenize(text, 128);
  int64_t seq_len = static_cast<int64_t>(
      tokens.input_ids.size());

  // 2. Create memory info for CPU
  OrtMemoryInfo* mem_info = nullptr;
  auto status = g_ort->CreateCpuMemoryInfo(
      OrtArenaAllocator, OrtMemTypeDefault, &mem_info);
  if (status) {
    g_ort->ReleaseStatus(status);
    return {};
  }

  // 3. Create input tensors
  int64_t shape[] = {1, seq_len};
  size_t data_size =
      static_cast<size_t>(seq_len) * sizeof(int64_t);

  OrtValue* input_ids_tensor = nullptr;
  OrtValue* attention_mask_tensor = nullptr;
  OrtValue* token_type_ids_tensor = nullptr;

  auto cleanup = [&]() {
    if (input_ids_tensor)
      g_ort->ReleaseValue(input_ids_tensor);
    if (attention_mask_tensor)
      g_ort->ReleaseValue(attention_mask_tensor);
    if (token_type_ids_tensor)
      g_ort->ReleaseValue(token_type_ids_tensor);
    g_ort->ReleaseMemoryInfo(mem_info);
  };

  status = g_ort->CreateTensorWithDataAsOrtValue(
      mem_info, tokens.input_ids.data(), data_size,
      shape, 2, ONNX_TENSOR_ELEMENT_DATA_TYPE_INT64,
      &input_ids_tensor);
  if (status) {
    g_ort->ReleaseStatus(status);
    cleanup();
    return {};
  }

  status = g_ort->CreateTensorWithDataAsOrtValue(
      mem_info, tokens.attention_mask.data(), data_size,
      shape, 2, ONNX_TENSOR_ELEMENT_DATA_TYPE_INT64,
      &attention_mask_tensor);
  if (status) {
    g_ort->ReleaseStatus(status);
    cleanup();
    return {};
  }

  status = g_ort->CreateTensorWithDataAsOrtValue(
      mem_info, tokens.token_type_ids.data(), data_size,
      shape, 2, ONNX_TENSOR_ELEMENT_DATA_TYPE_INT64,
      &token_type_ids_tensor);
  if (status) {
    g_ort->ReleaseStatus(status);
    cleanup();
    return {};
  }

  // 4. Run inference
  const char* input_names[] = {
      "input_ids", "attention_mask", "token_type_ids"};
  const char* output_names[] = {"last_hidden_state"};
  const OrtValue* inputs[] = {input_ids_tensor,
                              attention_mask_tensor,
                              token_type_ids_tensor};
  OrtValue* output = nullptr;

  status = g_ort->Run(
      static_cast<OrtSession*>(session_), nullptr,
      input_names, inputs, 3, output_names, 1, &output);

  cleanup();

  if (status) {
    LOG(ERROR) << "ORT Run failed: "
               << g_ort->GetErrorMessage(status);
    g_ort->ReleaseStatus(status);
    return {};
  }

  // 5. Get output data
  float* output_data = nullptr;
  status = g_ort->GetTensorMutableData(
      output, (void**)&output_data);
  if (status || !output_data) {
    if (status) g_ort->ReleaseStatus(status);
    g_ort->ReleaseValue(output);
    return {};
  }

  // 6. Mean pooling with attention mask
  auto embedding = MeanPooling(output_data,
                               static_cast<int>(seq_len),
                               kEmbeddingDim,
                               tokens.attention_mask);

  // 7. L2 normalize
  L2Normalize(embedding);

  g_ort->ReleaseValue(output);
  return embedding;
}

std::vector<float> OnDeviceEmbedding::MeanPooling(
    const float* output, int seq_len, int hidden_dim,
    const std::vector<int64_t>& attn_mask) {
  std::vector<float> result(hidden_dim, 0.0f);
  float mask_sum = 0.0f;

  for (int i = 0; i < seq_len; ++i) {
    float mask = static_cast<float>(attn_mask[i]);
    mask_sum += mask;
    for (int j = 0; j < hidden_dim; ++j) {
      result[j] += output[i * hidden_dim + j] * mask;
    }
  }

  if (mask_sum > 0) {
    for (int j = 0; j < hidden_dim; ++j) {
      result[j] /= mask_sum;
    }
  }

  return result;
}

void OnDeviceEmbedding::L2Normalize(
    std::vector<float>& vec) {
  float norm = 0.0f;
  for (float v : vec) norm += v * v;
  norm = std::sqrt(norm);

  if (norm > 1e-12f) {
    for (float& v : vec) v /= norm;
  }
}

}  // namespace tizenclaw
