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
#ifndef EMBEDDING_STORE_HH
#define EMBEDDING_STORE_HH

#include <sqlite3.h>

#include <cstdint>
#include <string>
#include <vector>

namespace tizenclaw {

// Embedding-based document store for RAG
// (Retrieval-Augmented Generation).
// Uses SQLite to store text chunks alongside
// their embedding vectors (as BLOBs).
// Search is brute-force cosine similarity
// (sufficient for embedded-scale corpora).
class EmbeddingStore {
 public:
  EmbeddingStore() = default;
  ~EmbeddingStore();

  // Open (or create) the SQLite database
  [[nodiscard]] bool Initialize(const std::string& db_path);
  void Close();

  // Store a document chunk with its embedding
  [[nodiscard]] bool StoreChunk(const std::string& source,
                                const std::string& chunk_text,
                                const std::vector<float>& embedding);

  // Semantic search: compare query_embedding
  // against all stored embeddings via cosine
  // similarity. Returns top_k results.
  struct SearchResult {
    std::string source;
    std::string chunk_text;
    float score;
  };
  [[nodiscard]] std::vector<SearchResult> Search(
      const std::vector<float>& query_embedding, int top_k = 5);

  // Hybrid search: combines BM25 keyword search
  // (via FTS5) with vector cosine similarity
  // using Reciprocal Rank Fusion (RRF).
  // Falls back to vector-only if FTS5 unavailable.
  [[nodiscard]] std::vector<SearchResult>
  HybridSearch(
      const std::string& query_text,
      const std::vector<float>& query_embedding,
      int top_k = 5);

  // Estimate token count for a text string.
  // Uses whitespace split * 1.3 factor.
  [[nodiscard]] static int EstimateTokens(
      const std::string& text);

  // Attach a pre-built knowledge database
  // (read-only, for RAG from Tizen docs etc.)
  [[nodiscard]] bool AttachKnowledgeDB(const std::string& path);

  // Register a knowledge DB path for lazy loading.
  // The DB will only be attached on first search.
  void RegisterKnowledgeDB(const std::string& path);

  // Detach all knowledge DBs to reclaim file cache.
  // Safe to call repeatedly; next search re-attaches.
  void DetachKnowledgeDBs();

  // Delete all chunks from a given source
  [[nodiscard]] bool DeleteSource(const std::string& source);

  // Total number of stored chunks
  [[nodiscard]] int GetChunkCount() const;

  // Chunk count from all attached knowledge DBs
  [[nodiscard]] int GetKnowledgeChunkCount();

  // Number of knowledge DBs registered (pending lazy load)
  [[nodiscard]] int GetPendingKnowledgeCount() const {
    return static_cast<int>(pending_paths_.size());
  }

  // --- Utility (public for testing) ---

  // Split text into ~chunk_size character chunks
  // with ~overlap overlap.
  [[nodiscard]] static std::vector<std::string> ChunkText(
      const std::string& text, size_t chunk_size = 500, size_t overlap = 50);

  // Cosine similarity between two vectors
  [[nodiscard]] static float CosineSimilarity(const std::vector<float>& a,
                                              const std::vector<float>& b);

 private:
  bool CreateTable();

  // Create FTS5 virtual table for keyword search
  bool CreateFtsTable();

  // Lazily attach all registered knowledge DBs.
  // Called internally before search operations.
  void EnsureKnowledgeAttached();

  // BLOB <-> float vector conversion
  static std::vector<uint8_t> FloatsToBlob(const std::vector<float>& v);
  static std::vector<float> BlobToFloats(const void* data, int size);

  sqlite3* db_ = nullptr;
  std::vector<std::string> knowledge_aliases_;

  // Paths registered for lazy loading
  std::vector<std::string> pending_paths_;
  bool knowledge_attached_ = false;
};

}  // namespace tizenclaw

#endif  // EMBEDDING_STORE_HH
