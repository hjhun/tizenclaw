# Tizen Docs & glibc Knowledge RAG Builder

Build a host-side Python script that indexes Tizen Native C API documentation and glibc man pages into a pre-built SQLite database, compatible with TizenClaw's existing [EmbeddingStore](file:///home/hjhun/samba/github/tizenclaw/src/tizenclaw/storage/embedding_store.hh#19-20). The DB is generated on the host PC and deployed to the device.

## User Review Required

> [!IMPORTANT]
> **Embedding API Key**: The builder script needs access to the Gemini API for embedding generation (`text-embedding-004`). The script will read the API key from `GEMINI_API_KEY` environment variable or `--api-key` CLI arg. Is this acceptable?

> [!IMPORTANT]
> **glibc man pages scope**: The host PC has ~2968 man3 pages, but most are Perl modules, not C. The script will filter to only include C-relevant man pages (e.g., `printf.3`, `malloc.3`, `socket.3` etc. — filtering out `.3pm`, `.3perl`, and other non-C pages). The heuristic: include `*.3.gz` and `*.3p.gz`, exclude `*.3pm.gz`, `*.3perl.gz`, etc.

> [!WARNING]
> **HTML API docs size**: The 437 `group__CAPI_*.html` files total ~41MB of raw HTML. After HTML-to-text extraction that strips boilerplate (nav, scripts, CSS), useful content will be ~5-10MB. With chunking at 500 chars and Gemini embedding API rate limits, the initial build may take **10-20 minutes**. The script will support `--resume` to continue from where it left off.

---

## Proposed Changes

### Host-side Builder Script

#### [NEW] [build_knowledge_db.py](file:///home/hjhun/samba/github/tizenclaw/tools/build_knowledge_db.py)

A standalone Python script that:

1. **Parses 3 content sources**:
   - **Tizen Native Guides** (`docs/application/native/guides/**/*.md`): Read as Markdown, strip YAML frontmatter
   - **Tizen Native C API Reference** (`docs/application/native/api/common/10.0/group__CAPI_*.html`): Parse HTML with `html.parser` (stdlib), extract title, overview, function signatures, parameter descriptions, return values, and remarks from Doxygen structure
   - **glibc man pages** (`/usr/share/man/man3/*.3.gz`): Decompress with `gzip`, parse `nroff`/`man` format using `subprocess` call to `man -l` or direct text extraction

2. **Chunks text** using sentence-boundary aware splitter (matching `EmbeddingStore::ChunkText` behavior — 500 chars, 50 char overlap)

3. **Generates embeddings** via Gemini API (`text-embedding-004` model, 768-dim) with:
   - Batching (up to 100 texts per request)
   - Rate limit handling with exponential backoff
   - Progress tracking and `--resume` support

4. **Writes SQLite DB** with the exact schema used by [EmbeddingStore](file:///home/hjhun/samba/github/tizenclaw/src/tizenclaw/storage/embedding_store.hh#19-20):
   ```sql
   CREATE TABLE IF NOT EXISTS embeddings (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     source TEXT NOT NULL,
     chunk_text TEXT NOT NULL,
     embedding BLOB NOT NULL
   );
   ```
   Where `embedding` is a packed `float[]` blob (`struct.pack('f' * dim, ...)`)

5. **CLI interface**:
   ```
   python3 tools/build_knowledge_db.py \
     --tizen-docs ~/samba/github/tizen-docs \
     --output data/tizen_knowledge.db \
     --api-key $GEMINI_API_KEY \
     --resume
   ```

**Dependencies**: Only Python 3 stdlib + `urllib.request` for Gemini API calls (no pip installs required).

---

### C++ Daemon Integration

#### [MODIFY] [agent_core.cc](file:///home/hjhun/samba/github/tizenclaw/src/tizenclaw/core/agent_core.cc)

In `AgentCore::Initialize()` (around line 188-201):
- After initializing the runtime `embeddings.db`, check for a pre-built knowledge DB at `APP_DATA_DIR/rag/tizen_knowledge.db`
- If found, attach it to the existing SQLite connection as a read-only secondary database
- Modify `search_knowledge` handler to search both the runtime DB and the pre-built knowledge DB

**Approach**: Use SQLite's `ATTACH DATABASE` to mount the knowledge DB as `knowledge`, then query with `UNION ALL` across both `main.embeddings` and `knowledge.embeddings`.

#### [MODIFY] [embedding_store.hh](file:///home/hjhun/samba/github/tizenclaw/src/tizenclaw/storage/embedding_store.hh)

Add:
- `bool AttachKnowledgeDB(const std::string& path)` — attaches a read-only pre-built DB
- Update [Search()](file:///home/hjhun/samba/github/tizenclaw/src/tizenclaw/storage/embedding_store.cc#121-181) to also search the attached knowledge DB
- `int GetKnowledgeChunkCount()` — returns count from attached DB

#### [MODIFY] [embedding_store.cc](file:///home/hjhun/samba/github/tizenclaw/src/tizenclaw/storage/embedding_store.cc)

Implement:
- `AttachKnowledgeDB`: `ATTACH DATABASE '...' AS knowledge`
- Modify [Search](file:///home/hjhun/samba/github/tizenclaw/src/tizenclaw/storage/embedding_store.cc#121-181) to load embeddings from both `main.embeddings` and `knowledge.embeddings`
- `GetKnowledgeChunkCount`: `SELECT COUNT(*) FROM knowledge.embeddings`

---

### Packaging & Deployment

#### [MODIFY] [tizenclaw.spec](file:///home/hjhun/samba/github/tizenclaw/packaging/tizenclaw.spec)

- Add `/opt/usr/share/tizenclaw/rag/` directory
- Include `data/tizen_knowledge.db` in the RPM package (if it exists at build time)

#### [NEW] [build_knowledge_db.sh](file:///home/hjhun/samba/github/tizenclaw/tools/build_knowledge_db.sh)

Convenience wrapper script:
```bash
#!/bin/bash
# Build the Tizen knowledge RAG database
python3 "$(dirname "$0")/build_knowledge_db.py" \
  --tizen-docs "${TIZEN_DOCS_PATH:-$HOME/samba/github/tizen-docs}" \
  --output "$(dirname "$0")/../data/tizen_knowledge.db" \
  "$@"
```

---

## Verification Plan

### Automated Tests

1. **Unit test — `EmbeddingStore::AttachKnowledgeDB`**:
   Extend existing [test/unit_tests/embedding_store_test.cc](file:///home/hjhun/samba/github/tizenclaw/test/unit_tests/embedding_store_test.cc).
   ```
   cd /home/hjhun/samba/github/tizenclaw && gbs build -A aarch64
   # or run locally:
   mkdir -p build && cd build && cmake .. && make && ctest -V -R EmbeddingStore
   ```

2. **Python builder script dry-run test**:
   ```
   python3 tools/build_knowledge_db.py \
     --tizen-docs ~/samba/github/tizen-docs \
     --output /tmp/test_knowledge.db \
     --dry-run
   ```
   This should list all discovered files without making API calls, and validate parsing.

3. **SQLite DB schema validation**:
   ```
   sqlite3 /tmp/test_knowledge.db ".schema" 
   # Should show the embeddings table with id, source, chunk_text, embedding columns
   ```

### Manual Verification

1. **Full build with API key**: Run the builder script with a real Gemini API key and verify embeddings are generated:
   ```
   GEMINI_API_KEY=<key> python3 tools/build_knowledge_db.py \
     --tizen-docs ~/samba/github/tizen-docs \
     --output data/tizen_knowledge.db
   ```
   Check output stats (chunk count, DB size).

2. **Device deployment**: Deploy the built RPM to the device, restart tizenclaw, and test:
   ```
   tizenclaw-cli "How do I use app_control_send_launch_request?"
   ```
   The response should reference Tizen API documentation details from the knowledge DB.

3. **gbs build passes**: `gbs build -A aarch64` should complete without errors.
