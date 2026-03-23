# ML/AI Assets (RAG, OCR, ONNX Runtime)

TizenClaw uses an **on-device embedding** system for RAG that is fully independent of the LLM backend. This ensures consistent semantic search results regardless of which LLM (Gemini, OpenAI, Ollama, etc.) is active.

## Architecture

```
┌─────────────────────────────────┐    ┌─────────────────────────────────┐
│     Build Time (Host PC)        │    │      Runtime (Device)           │
│                                 │    │                                 │
│  Python + sentence-transformers │    │  C++ + ONNX Runtime (dlopen)   │
│  all-MiniLM-L6-v2 → 384-dim    │    │  all-MiniLM-L6-v2 → 384-dim   │
│           ↓                     │    │           ↓                     │
│  tizen_api.db   (43 MB)        │    │  Query embedding generation     │
│  tizen_guide.db (19 MB)        │    │  Cosine similarity search       │
│           ↓                     │    │           ↓                     │
│  RPM install ──────────────────────→ /opt/usr/share/tizenclaw/rag/    │
└─────────────────────────────────┘    └─────────────────────────────────┘
```

## Companion Project: tizenclaw-assets

RAG assets are part of the consolidated **[tizenclaw-assets](https://github.com/hjhun/tizenclaw-assets)** package, which also includes ONNX Runtime, the OCR engine, and the embedding model.

The project produces an independent RPM containing:

| Component | Install Path | Size |
|-----------|-------------|------|
| Knowledge Databases | `/opt/usr/share/tizenclaw/rag/` | ~62 MB |
| ONNX Runtime | `/opt/usr/share/tizenclaw/lib/` | ~16 MB |
| Embedding Model | `/opt/usr/share/tizenclaw/models/all-MiniLM-L6-v2/` | ~90 MB |
| OCR Models (PP-OCRv3) | `/opt/usr/share/tizenclaw/models/ppocr/` | ~15 MB (lite) / ~86 MB (full) |
| OCR CLI Tool | `/opt/usr/share/tizen-tools/cli/tizenclaw-ocr/` | ~130 KB |

### Knowledge Databases

| Database | Source | Files | Chunks |
|----------|--------|------:|-------:|
| `tizen_api.db` | Native C-API Doxygen (HTML) | 437 | 10,915 |
| `tizen_guide.db` | Native Guides (Markdown) | 302 | 4,770 |

### Building and Deploying

```bash
# Option A: Automatic (via deploy.sh)
# deploy.sh auto-detects ../tizenclaw-assets and builds it alongside tizenclaw
./deploy.sh

# Option B: Manual
cd ../tizenclaw-assets
gbs build -A x86_64 --include-all

# Option C: With full CJK OCR model (default is lite Korean+English)
cd ../tizenclaw-assets
gbs build -A x86_64 --include-all --define "ocr_model full"
```

### Regenerating RAG Databases

If you need to rebuild the knowledge databases from source documentation:

```bash
cd ../tizenclaw-assets

# Auto-download tizen-docs from GitHub (if not present)
./scripts/setup_docs.sh

# Build databases using local embeddings (no API key needed)
pip3 install sentence-transformers
./scripts/build_knowledge_db.sh
```

## On-Device Embedding Module

The C++ embedding module (`src/tizenclaw/embedding/`) remains in the main tizenclaw project:

| File | Description |
|------|-------------|
| `wordpiece_tokenizer.{hh,cc}` | BERT-compatible WordPiece tokenizer |
| `on_device_embedding.{hh,cc}` | ONNX Runtime inference (dlopen) for `all-MiniLM-L6-v2` |
| `onnxruntime_c_api.h` | Official ONNX Runtime C API header (v1.20.1) |

### How It Works

1. **Tokenization** — Input text is tokenized using WordPiece vocabulary (`vocab.txt`, 30,522 tokens)
2. **Inference** — ONNX Runtime runs the `all-MiniLM-L6-v2` model producing per-token hidden states
3. **Pooling** — Mean pooling with attention mask produces a 384-dimensional embedding
4. **Normalization** — L2 normalization for cosine similarity compatibility
5. **Search** — Brute-force cosine similarity against pre-computed embeddings in SQLite

### EmbeddingStore Multi-DB Support

`EmbeddingStore` supports attaching multiple knowledge databases simultaneously:

```
embeddings.db (main store)
  ↓ ATTACH
knowledge_0 → tizen_guide.db
knowledge_1 → tizen_knowledge.db
knowledge_2 → tizen_api.db
```

Queries search across all attached databases and return the top-k most similar results.

## Supported Architectures

| Architecture | ONNX Runtime | Status |
|:---:|:---:|:---:|
| x86_64 | ✅ Prebuilt | Fully supported |
| aarch64 | ✅ Prebuilt | Fully supported |
| armv7l | ✅ Cross-compiled | Fully supported |

The armv7l library is cross-compiled from source using `arm-linux-gnueabihf-gcc`. To rebuild:
```bash
cd ../tizenclaw-assets
bash scripts/build_ort_armv7l.sh ~/path/to/onnxruntime
```
