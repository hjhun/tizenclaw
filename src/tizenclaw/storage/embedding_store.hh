#ifndef TIZENCLAW_STORAGE_EMBEDDING_STORE_H_
#define TIZENCLAW_STORAGE_EMBEDDING_STORE_H_

#include <string>
#include <vector>
#include <cstdint>
#include <sqlite3.h>

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
  [[nodiscard]] bool Initialize(
      const std::string& db_path);
  void Close();

  // Store a document chunk with its embedding
  [[nodiscard]] bool StoreChunk(
      const std::string& source,
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
      const std::vector<float>& query_embedding,
      int top_k = 5) const;

  // Attach a pre-built knowledge database
  // (read-only, for RAG from Tizen docs etc.)
  [[nodiscard]] bool AttachKnowledgeDB(
      const std::string& path);

  // Delete all chunks from a given source
  [[nodiscard]] bool DeleteSource(
      const std::string& source);

  // Total number of stored chunks
  [[nodiscard]] int GetChunkCount() const;

  // Chunk count from attached knowledge DB
  [[nodiscard]] int GetKnowledgeChunkCount() const;

  // --- Utility (public for testing) ---

  // Split text into ~chunk_size character chunks
  // with ~overlap overlap.
  [[nodiscard]] static std::vector<std::string>
  ChunkText(
      const std::string& text,
      size_t chunk_size = 500,
      size_t overlap = 50);

  // Cosine similarity between two vectors
  [[nodiscard]] static float CosineSimilarity(
      const std::vector<float>& a,
      const std::vector<float>& b);

private:
  bool CreateTable();

  // BLOB <-> float vector conversion
  static std::vector<uint8_t> FloatsToBlob(
      const std::vector<float>& v);
  static std::vector<float> BlobToFloats(
      const void* data, int size);

  sqlite3* db_ = nullptr;
  bool knowledge_attached_ = false;
};

}  // namespace tizenclaw

#endif  // TIZENCLAW_STORAGE_EMBEDDING_STORE_H_
