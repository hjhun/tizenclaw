#include "embedding_store.hh"
#include "../../common/logging.hh"

#include <cmath>
#include <cstring>
#include <algorithm>
#include <numeric>
#include <sstream>

namespace tizenclaw {

EmbeddingStore::~EmbeddingStore() {
  Close();
}

bool EmbeddingStore::Initialize(
    const std::string& db_path) {
  if (db_) {
    Close();
  }

  int rc = sqlite3_open(db_path.c_str(), &db_);
  if (rc != SQLITE_OK) {
    LOG(ERROR) << "Failed to open SQLite DB: "
               << db_path << " — "
               << sqlite3_errmsg(db_);
    db_ = nullptr;
    return false;
  }

  // Enable WAL mode for concurrent readers
  sqlite3_exec(db_, "PRAGMA journal_mode=WAL;",
               nullptr, nullptr, nullptr);

  if (!CreateTable()) {
    Close();
    return false;
  }

  LOG(INFO) << "EmbeddingStore initialized: "
            << db_path;
  return true;
}

void EmbeddingStore::Close() {
  if (db_) {
    if (knowledge_attached_) {
      sqlite3_exec(db_,
          "DETACH DATABASE knowledge;",
          nullptr, nullptr, nullptr);
      knowledge_attached_ = false;
    }
    sqlite3_close(db_);
    db_ = nullptr;
  }
}

bool EmbeddingStore::CreateTable() {
  const char* sql =
      "CREATE TABLE IF NOT EXISTS documents ("
      "  id INTEGER PRIMARY KEY AUTOINCREMENT,"
      "  source TEXT NOT NULL,"
      "  chunk_text TEXT NOT NULL,"
      "  embedding BLOB NOT NULL,"
      "  created_at TEXT DEFAULT "
      "    (datetime('now'))"
      ");";

  char* err = nullptr;
  int rc = sqlite3_exec(
      db_, sql, nullptr, nullptr, &err);
  if (rc != SQLITE_OK) {
    LOG(ERROR) << "Failed to create table: "
               << (err ? err : "unknown");
    sqlite3_free(err);
    return false;
  }
  return true;
}

bool EmbeddingStore::StoreChunk(
    const std::string& source,
    const std::string& chunk_text,
    const std::vector<float>& embedding) {
  if (!db_) return false;

  const char* sql =
      "INSERT INTO documents "
      "(source, chunk_text, embedding) "
      "VALUES (?, ?, ?);";

  sqlite3_stmt* stmt = nullptr;
  int rc = sqlite3_prepare_v2(
      db_, sql, -1, &stmt, nullptr);
  if (rc != SQLITE_OK) {
    LOG(ERROR) << "Prepare failed: "
               << sqlite3_errmsg(db_);
    return false;
  }

  sqlite3_bind_text(
      stmt, 1, source.c_str(),
      static_cast<int>(source.size()),
      SQLITE_TRANSIENT);
  sqlite3_bind_text(
      stmt, 2, chunk_text.c_str(),
      static_cast<int>(chunk_text.size()),
      SQLITE_TRANSIENT);

  auto blob = FloatsToBlob(embedding);
  sqlite3_bind_blob(
      stmt, 3, blob.data(),
      static_cast<int>(blob.size()),
      SQLITE_TRANSIENT);

  rc = sqlite3_step(stmt);
  sqlite3_finalize(stmt);

  if (rc != SQLITE_DONE) {
    LOG(ERROR) << "Insert failed: "
               << sqlite3_errmsg(db_);
    return false;
  }
  return true;
}

std::vector<EmbeddingStore::SearchResult>
EmbeddingStore::Search(
    const std::vector<float>& query_embedding,
    int top_k) const {
  std::vector<SearchResult> results;
  if (!db_ || query_embedding.empty()) {
    return results;
  }

  // Collect all results with scores
  std::vector<SearchResult> all;

  // Lambda to scan a table
  auto scan_table = [&](
      const char* table_sql) {
    sqlite3_stmt* stmt = nullptr;
    int rc = sqlite3_prepare_v2(
        db_, table_sql, -1, &stmt, nullptr);
    if (rc != SQLITE_OK) return;

    while (sqlite3_step(stmt) == SQLITE_ROW) {
      SearchResult r;
      const char* src =
          reinterpret_cast<const char*>(
              sqlite3_column_text(stmt, 0));
      const char* txt =
          reinterpret_cast<const char*>(
              sqlite3_column_text(stmt, 1));
      r.source = src ? src : "";
      r.chunk_text = txt ? txt : "";

      const void* blob_data =
          sqlite3_column_blob(stmt, 2);
      int blob_size =
          sqlite3_column_bytes(stmt, 2);
      auto emb = BlobToFloats(
          blob_data, blob_size);

      r.score = CosineSimilarity(
          query_embedding, emb);
      all.push_back(std::move(r));
    }
    sqlite3_finalize(stmt);
  };

  // Scan main (runtime) documents
  scan_table(
      "SELECT source, chunk_text, embedding "
      "FROM documents;");

  // Scan attached knowledge DB if available
  if (knowledge_attached_) {
    scan_table(
        "SELECT source, chunk_text, embedding "
        "FROM knowledge.documents;");
  }


  // Sort by descending score
  std::sort(all.begin(), all.end(),
      [](const SearchResult& a,
         const SearchResult& b) {
        return a.score > b.score;
      });

  // Return top_k
  int count = std::min(
      top_k, static_cast<int>(all.size()));
  results.assign(
      all.begin(), all.begin() + count);
  return results;
}

bool EmbeddingStore::DeleteSource(
    const std::string& source) {
  if (!db_) return false;

  const char* sql =
      "DELETE FROM documents WHERE source = ?;";

  sqlite3_stmt* stmt = nullptr;
  int rc = sqlite3_prepare_v2(
      db_, sql, -1, &stmt, nullptr);
  if (rc != SQLITE_OK) return false;

  sqlite3_bind_text(
      stmt, 1, source.c_str(),
      static_cast<int>(source.size()),
      SQLITE_TRANSIENT);

  rc = sqlite3_step(stmt);
  sqlite3_finalize(stmt);
  return rc == SQLITE_DONE;
}

int EmbeddingStore::GetChunkCount() const {
  if (!db_) return 0;

  const char* sql =
      "SELECT COUNT(*) FROM documents;";
  sqlite3_stmt* stmt = nullptr;
  int rc = sqlite3_prepare_v2(
      db_, sql, -1, &stmt, nullptr);
  if (rc != SQLITE_OK) return 0;

  int count = 0;
  if (sqlite3_step(stmt) == SQLITE_ROW) {
    count = sqlite3_column_int(stmt, 0);
  }
  sqlite3_finalize(stmt);
  return count;
}

bool EmbeddingStore::AttachKnowledgeDB(
    const std::string& path) {
  if (!db_) return false;

  // Check file exists
  FILE* f = fopen(path.c_str(), "r");
  if (!f) {
    LOG(WARNING) << "Knowledge DB not found: "
                 << path;
    return false;
  }
  fclose(f);

  std::string sql =
      "ATTACH DATABASE '" + path +
      "' AS knowledge;";
  char* err = nullptr;
  int rc = sqlite3_exec(
      db_, sql.c_str(),
      nullptr, nullptr, &err);
  if (rc != SQLITE_OK) {
    LOG(ERROR) << "Failed to attach knowledge "
               << "DB: " << (err ? err : "?");
    sqlite3_free(err);
    return false;
  }

  knowledge_attached_ = true;
  LOG(INFO) << "Knowledge DB attached: " << path
            << " (" << GetKnowledgeChunkCount()
            << " chunks)";
  return true;
}

int EmbeddingStore::GetKnowledgeChunkCount() const {
  if (!db_ || !knowledge_attached_) return 0;

  const char* sql =
      "SELECT COUNT(*) FROM "
      "knowledge.documents;";
  sqlite3_stmt* stmt = nullptr;
  int rc = sqlite3_prepare_v2(
      db_, sql, -1, &stmt, nullptr);
  if (rc != SQLITE_OK) return 0;

  int count = 0;
  if (sqlite3_step(stmt) == SQLITE_ROW) {
    count = sqlite3_column_int(stmt, 0);
  }
  sqlite3_finalize(stmt);
  return count;
}

// --- Text chunking ---

std::vector<std::string>
EmbeddingStore::ChunkText(
    const std::string& text,
    size_t chunk_size,
    size_t overlap) {
  std::vector<std::string> chunks;
  if (text.empty() || chunk_size == 0) {
    return chunks;
  }

  size_t pos = 0;
  while (pos < text.size()) {
    size_t end = std::min(
        pos + chunk_size, text.size());

    // Try to break at a sentence boundary
    if (end < text.size()) {
      size_t last_period =
          text.rfind('.', end);
      if (last_period != std::string::npos &&
          last_period > pos + chunk_size / 2) {
        end = last_period + 1;
      }
    }

    chunks.push_back(
        text.substr(pos, end - pos));

    if (end >= text.size()) break;

    // Next chunk starts with overlap
    pos = (end > overlap) ?
        end - overlap : end;
  }
  return chunks;
}

// --- Cosine similarity ---

float EmbeddingStore::CosineSimilarity(
    const std::vector<float>& a,
    const std::vector<float>& b) {
  if (a.size() != b.size() || a.empty()) {
    return 0.0f;
  }

  float dot = 0.0f;
  float norm_a = 0.0f;
  float norm_b = 0.0f;

  for (size_t i = 0; i < a.size(); ++i) {
    dot += a[i] * b[i];
    norm_a += a[i] * a[i];
    norm_b += b[i] * b[i];
  }

  float denom = std::sqrt(norm_a) *
                std::sqrt(norm_b);
  if (denom < 1e-10f) return 0.0f;

  return dot / denom;
}

// --- BLOB <-> float conversion ---

std::vector<uint8_t>
EmbeddingStore::FloatsToBlob(
    const std::vector<float>& v) {
  std::vector<uint8_t> blob(
      v.size() * sizeof(float));
  std::memcpy(blob.data(), v.data(),
              blob.size());
  return blob;
}

// --- Float16 decoding ---
static float HalfToFloat(uint16_t h) {
  uint32_t sign = (h >> 15) & 0x00000001;
  uint32_t exponent = (h >> 10) & 0x0000001f;
  uint32_t mantissa = h & 0x000003ff;

  if (exponent == 0) {
    if (mantissa == 0) {
      uint32_t res = sign << 31;
      float f; std::memcpy(&f, &res, 4);
      return f;
    } else {
      while (!(mantissa & 0x00000400)) {
        mantissa <<= 1;
        exponent -= 1;
      }
      exponent += 1;
      mantissa &= ~0x00000400;
    }
  } else if (exponent == 31) {
    if (mantissa == 0) {
      uint32_t res = (sign << 31) | 0x7f800000;
      float f; std::memcpy(&f, &res, 4);
      return f;
    } else {
      uint32_t res = (sign << 31) | 0x7f800000 | (mantissa << 13);
      float f; std::memcpy(&f, &res, 4);
      return f;
    }
  }

  exponent = exponent + (127 - 15);
  mantissa = mantissa << 13;
  uint32_t res = (sign << 31) | (exponent << 23) | mantissa;
  float f; std::memcpy(&f, &res, 4);
  return f;
}

std::vector<float>
EmbeddingStore::BlobToFloats(
    const void* data, int size) {
  if (!data || size <= 0) return {};

  int num_elements = 0;
  if (size % sizeof(float) == 0) {
    // Float32 processing (3072 bytes for 768-dim)
    num_elements = size / sizeof(float);
    std::vector<float> vec(num_elements);
    std::memcpy(vec.data(), data, size);
    return vec;
  } else if (size % 2 == 0) {
    // Float16 processing (1536 bytes for 768-dim)
    num_elements = size / 2;
    std::vector<float> vec(num_elements);
    const uint16_t* u16_data = static_cast<const uint16_t*>(data);
    for (int i = 0; i < num_elements; ++i) {
      vec[i] = HalfToFloat(u16_data[i]);
    }
    return vec;
  }

  return {};
}

}  // namespace tizenclaw
