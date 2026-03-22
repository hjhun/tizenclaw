import sqlite3
import math
import struct
import logging
from typing import List, Dict, Tuple

logger = logging.getLogger(__name__)

class SearchResult:
    def __init__(self, source: str, chunk_text: str, score: float):
        self.source = source
        self.chunk_text = chunk_text
        self.score = score

class EmbeddingStore:
    """
    Python implementation of TizenClaw EmbeddingStore.
    Uses sqlite3 to store chunks and embeddings (as BLOBs).
    """
    def __init__(self):
        self.db: sqlite3.Connection = None
        self.knowledge_aliases: List[str] = []

    def initialize(self, db_path: str) -> bool:
        try:
            self.db = sqlite3.connect(db_path)
            self._create_tables()
            logger.info(f"Connected to embedding DB: {db_path}")
            return True
        except Exception as e:
            logger.error(f"Failed to initialize embedding DB: {e}")
            return False

    def close(self):
        if self.db:
            self.db.close()
            self.db = None

    def _create_tables(self):
        cursor = self.db.cursor()
        cursor.execute('''
            CREATE TABLE IF NOT EXISTS embeddings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source TEXT NOT NULL,
                chunk_text TEXT NOT NULL,
                embedding BLOB NOT NULL
            )
        ''')
        cursor.execute('''
            CREATE VIRTUAL TABLE IF NOT EXISTS fts_embeddings USING fts5(
                source, chunk_text
            )
        ''')
        self.db.commit()

    def store_chunk(self, source: str, chunk_text: str, embedding: List[float]) -> bool:
        try:
            blob = struct.pack(f'{len(embedding)}f', *embedding)
            cursor = self.db.cursor()
            cursor.execute(
                "INSERT INTO embeddings (source, chunk_text, embedding) VALUES (?, ?, ?)",
                (source, chunk_text, blob)
            )
            cursor.execute(
                "INSERT INTO fts_embeddings (source, chunk_text) VALUES (?, ?)",
                (source, chunk_text)
            )
            self.db.commit()
            return True
        except Exception as e:
            logger.error(f"Failed to store chunk: {e}")
            return False

    def search(self, query_embedding: List[float], top_k: int = 5) -> List[SearchResult]:
        if not self.db:
            return []
        cursor = self.db.cursor()
        cursor.execute("SELECT source, chunk_text, embedding FROM embeddings")
        results = []
        for row in cursor.fetchall():
            source, chunk_text, blob = row
            embedding = list(struct.unpack(f'{len(query_embedding)}f', blob))
            score = self.cosine_similarity(query_embedding, embedding)
            results.append((source, chunk_text, score))
        
        results.sort(key=lambda x: x[2], reverse=True)
        return [SearchResult(s, c, sc) for s, c, sc in results[:top_k]]

    def hybrid_search(self, query_text: str, query_embedding: List[float], top_k: int = 5) -> List[SearchResult]:
        # Placeholder for Reciprocal Rank Fusion of FTS5 and vector search
        return self.search(query_embedding, top_k)

    @staticmethod
    def estimate_tokens(text: str) -> int:
        return int(len(text.split()) * 1.3)

    def attach_knowledge_db(self, path: str) -> bool:
        try:
            alias = f"knowledgedb_{len(self.knowledge_aliases)}"
            self.db.execute(f"ATTACH DATABASE '{path}' AS {alias}")
            self.knowledge_aliases.append(alias)
            return True
        except Exception as e:
            logger.error(f"Failed to attach knowledge DB: {e}")
            return False

    def delete_source(self, source: str) -> bool:
        if not self.db: return False
        try:
            self.db.execute("DELETE FROM embeddings WHERE source = ?", (source,))
            self.db.execute("DELETE FROM fts_embeddings WHERE source = ?", (source,))
            self.db.commit()
            return True
        except Exception as e:
            logger.error(f"Failed to delete source {source}: {e}")
            return False

    def get_chunk_count(self) -> int:
        if not self.db: return 0
        cursor = self.db.cursor()
        cursor.execute("SELECT COUNT(*) FROM embeddings")
        return cursor.fetchone()[0]

    def get_knowledge_chunk_count(self) -> int:
        count = 0
        for alias in self.knowledge_aliases:
            try:
                cursor = self.db.cursor()
                cursor.execute(f"SELECT COUNT(*) FROM {alias}.embeddings")
                count += cursor.fetchone()[0]
            except Exception:
                pass
        return count

    @staticmethod
    def chunk_text(text: str, chunk_size: int = 500, overlap: int = 50) -> List[str]:
        chunks = []
        start = 0
        while start < len(text):
            chunks.append(text[start:start + chunk_size])
            start += chunk_size - overlap
        return chunks

    @staticmethod
    def cosine_similarity(vec1: List[float], vec2: List[float]) -> float:
        dot_product = sum(a * b for a, b in zip(vec1, vec2))
        norm_a = math.sqrt(sum(a * a for a in vec1))
        norm_b = math.sqrt(sum(b * b for b in vec2))
        if norm_a == 0 or norm_b == 0:
            return 0.0
        return dot_product / (norm_a * norm_b)
