#include <gtest/gtest.h>

#include "embedding_store.hh"

#include <cstdio>
#include <cmath>
#include <cstring>

using namespace tizenclaw;

class EmbeddingStoreTest : public ::testing::Test {
protected:
  void SetUp() override {
    db_path_ = "/tmp/test_embeddings.db";
    std::remove(db_path_.c_str());
  }

  void TearDown() override {
    store_.Close();
    std::remove(db_path_.c_str());
  }

  EmbeddingStore store_;
  std::string db_path_;
};

TEST_F(EmbeddingStoreTest, InitializeAndClose) {
  EXPECT_TRUE(store_.Initialize(db_path_));
  EXPECT_EQ(store_.GetChunkCount(), 0);
  store_.Close();
}

TEST_F(EmbeddingStoreTest, StoreAndCount) {
  ASSERT_TRUE(store_.Initialize(db_path_));

  std::vector<float> emb = {
      0.1f, 0.2f, 0.3f, 0.4f};
  EXPECT_TRUE(
      store_.StoreChunk("test", "hello", emb));
  EXPECT_EQ(store_.GetChunkCount(), 1);

  EXPECT_TRUE(
      store_.StoreChunk("test", "world", emb));
  EXPECT_EQ(store_.GetChunkCount(), 2);
}

TEST_F(EmbeddingStoreTest, SearchTopK) {
  ASSERT_TRUE(store_.Initialize(db_path_));

  // Store 3 chunks with different embeddings
  std::vector<float> emb1 = {1, 0, 0, 0};
  std::vector<float> emb2 = {0, 1, 0, 0};
  std::vector<float> emb3 = {0.9f, 0.1f, 0, 0};

  ASSERT_TRUE(store_.StoreChunk("doc1", "chunk1", emb1));
  ASSERT_TRUE(store_.StoreChunk("doc2", "chunk2", emb2));
  ASSERT_TRUE(store_.StoreChunk("doc3", "chunk3", emb3));

  // Search with query similar to emb1
  std::vector<float> query = {1, 0, 0, 0};
  auto results = store_.Search(query, 2);

  ASSERT_EQ(results.size(), 2u);
  // chunk1 should be first (exact match)
  EXPECT_EQ(results[0].chunk_text, "chunk1");
  EXPECT_NEAR(results[0].score, 1.0f, 0.01f);
  // chunk3 should be second (most similar)
  EXPECT_EQ(results[1].chunk_text, "chunk3");
}

TEST_F(EmbeddingStoreTest, DeleteSource) {
  ASSERT_TRUE(store_.Initialize(db_path_));

  std::vector<float> emb = {1, 0, 0};
  ASSERT_TRUE(store_.StoreChunk("src1", "a", emb));
  ASSERT_TRUE(store_.StoreChunk("src1", "b", emb));
  ASSERT_TRUE(store_.StoreChunk("src2", "c", emb));
  EXPECT_EQ(store_.GetChunkCount(), 3);

  EXPECT_TRUE(store_.DeleteSource("src1"));
  EXPECT_EQ(store_.GetChunkCount(), 1);
}

TEST_F(EmbeddingStoreTest,
       CosineSimilarityIdentical) {
  std::vector<float> a = {1, 2, 3};
  float sim =
      EmbeddingStore::CosineSimilarity(a, a);
  EXPECT_NEAR(sim, 1.0f, 0.001f);
}

TEST_F(EmbeddingStoreTest,
       CosineSimilarityOrthogonal) {
  std::vector<float> a = {1, 0, 0};
  std::vector<float> b = {0, 1, 0};
  float sim =
      EmbeddingStore::CosineSimilarity(a, b);
  EXPECT_NEAR(sim, 0.0f, 0.001f);
}

TEST_F(EmbeddingStoreTest,
       CosineSimilarityDifferentSize) {
  std::vector<float> a = {1, 2};
  std::vector<float> b = {1, 2, 3};
  // Different sizes → 0
  EXPECT_NEAR(
      EmbeddingStore::CosineSimilarity(a, b),
      0.0f, 0.001f);
}

TEST_F(EmbeddingStoreTest, AttachKnowledgeDB) {
  // 1. Create a "knowledge" DB directly with sqlite3
  std::string knowledge_db_path = "/tmp/test_knowledge.db";
  std::remove(knowledge_db_path.c_str());

  sqlite3* kdb = nullptr;
  ASSERT_EQ(sqlite3_open(knowledge_db_path.c_str(), &kdb), SQLITE_OK);
  const char* sql =
      "CREATE TABLE documents ("
      "  id INTEGER PRIMARY KEY AUTOINCREMENT,"
      "  source TEXT NOT NULL,"
      "  chunk_text TEXT NOT NULL,"
      "  embedding BLOB NOT NULL,"
      "  created_at TEXT DEFAULT (datetime('now'))"
      ");";
  ASSERT_EQ(sqlite3_exec(kdb, sql, nullptr, nullptr, nullptr), SQLITE_OK);

  // Insert a mock embedding into the knowledge DB
  std::vector<float> k_emb = {0, 0, 1, 0};
  std::vector<uint8_t> k_blob(k_emb.size() * sizeof(float));
  std::memcpy(k_blob.data(), k_emb.data(), k_blob.size());

  sqlite3_stmt* stmt = nullptr;
  ASSERT_EQ(sqlite3_prepare_v2(kdb,
      "INSERT INTO documents (source, chunk_text, embedding) VALUES (?, ?, ?);",
      -1, &stmt, nullptr), SQLITE_OK);
  sqlite3_bind_text(stmt, 1, "k_doc1", 6, SQLITE_TRANSIENT);
  sqlite3_bind_text(stmt, 2, "k_chunk1", 8, SQLITE_TRANSIENT);
  sqlite3_bind_blob(stmt, 3, k_blob.data(), k_blob.size(), SQLITE_TRANSIENT);
  ASSERT_EQ(sqlite3_step(stmt), SQLITE_DONE);
  sqlite3_finalize(stmt);
  sqlite3_close(kdb);

  // 2. Initialize the main store and store a chunk
  ASSERT_TRUE(store_.Initialize(db_path_));
  std::vector<float> m_emb = {1, 0, 0, 0};
  ASSERT_TRUE(store_.StoreChunk("m_doc1", "m_chunk1", m_emb));

  // 3. Attach the knowledge DB
  EXPECT_TRUE(store_.AttachKnowledgeDB(knowledge_db_path));
  EXPECT_EQ(store_.GetKnowledgeChunkCount(), 1);

  // 4. Search should find from both
  // Query matching main DB
  std::vector<float> q1 = {1, 0, 0, 0};
  auto r1 = store_.Search(q1, 5);
  ASSERT_GE(r1.size(), 1u);
  EXPECT_EQ(r1[0].source, "m_doc1");

  // Query matching knowledge DB
  std::vector<float> q2 = {0, 0, 1, 0};
  auto r2 = store_.Search(q2, 5);
  ASSERT_GE(r2.size(), 1u);
  EXPECT_EQ(r2[0].source, "k_doc1");

  // Cleanup
  std::remove(knowledge_db_path.c_str());
}

TEST_F(EmbeddingStoreTest, ChunkTextBasic) {
  std::string text =
      "Hello world. This is a test. End.";
  auto chunks =
      EmbeddingStore::ChunkText(text, 20, 5);
  EXPECT_GT(chunks.size(), 0u);
  // All text should be covered
  for (const auto& c : chunks) {
    EXPECT_FALSE(c.empty());
  }
}

TEST_F(EmbeddingStoreTest, ChunkTextEmpty) {
  auto chunks =
      EmbeddingStore::ChunkText("", 500, 50);
  EXPECT_EQ(chunks.size(), 0u);
}
