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
#ifndef ON_DEVICE_EMBEDDING_HH
#define ON_DEVICE_EMBEDDING_HH

#include <cstdint>
#include <string>
#include <vector>

#include "wordpiece_tokenizer.hh"

namespace tizenclaw {

// On-device embedding using all-MiniLM-L6-v2 ONNX model.
// Loads ONNX Runtime via dlopen for graceful fallback.
// Generates 384-dim embeddings independently of LLM backend.
class OnDeviceEmbedding {
 public:
  OnDeviceEmbedding() = default;
  ~OnDeviceEmbedding();

  // Initialize: load ONNX Runtime, model, and vocab
  // model_dir should contain model.onnx and vocab.txt
  // ort_lib_path should point to libonnxruntime.so
  [[nodiscard]] bool Initialize(
      const std::string& model_dir,
      const std::string& ort_lib_path =
          "/opt/usr/share/tizenclaw/lib/"
          "libonnxruntime.so");

  void Shutdown();

  // Generate embedding for text (384-dim vector)
  [[nodiscard]] std::vector<float> Encode(const std::string& text);

  // Check if ONNX Runtime is available and model loaded
  bool IsAvailable() const { return session_ != nullptr; }

  // Embedding dimension (384 for all-MiniLM-L6-v2)
  static constexpr int kEmbeddingDim = 384;

 private:
  // Mean pooling over token embeddings with attention mask
  static std::vector<float> MeanPooling(const float* output, int seq_len,
                                        int hidden_dim,
                                        const std::vector<int64_t>& attn_mask);

  // L2 normalize a vector
  static void L2Normalize(std::vector<float>& vec);

  WordPieceTokenizer tokenizer_;

  // ONNX Runtime loaded via dlopen
  void* ort_lib_ = nullptr;

  // Opaque ORT handles
  void* env_ = nullptr;
  void* session_ = nullptr;
  void* session_options_ = nullptr;
  void* allocator_ = nullptr;

  // ORT C API function pointers (minimal set)
  struct OrtApi;
  const OrtApi* api_ = nullptr;

};

}  // namespace tizenclaw

#endif  // ON_DEVICE_EMBEDDING_HH
