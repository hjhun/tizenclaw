# ML/AI 에셋 (RAG, OCR, ONNX Runtime)

> **최종 업데이트**: 2026-03-22

TizenClaw는 LLM 백엔드와 완전히 독립적인 **온디바이스 임베딩** 시스템을 사용합니다. 어떤 LLM (Gemini, OpenAI, Ollama 등)이 활성화되어 있든 일관된 시맨틱 검색 결과를 보장합니다.

## 아키텍처

```
┌─────────────────────────────────┐    ┌─────────────────────────────────┐
│     빌드 타임 (호스트 PC)        │    │      런타임 (디바이스)           │
│                                 │    │                                 │
│  Python + sentence-transformers │    │  C++ + ONNX Runtime (dlopen)    │
│  all-MiniLM-L6-v2 → 384차원    │    │  all-MiniLM-L6-v2 → 384차원    │
│           ↓                     │    │           ↓                     │
│  tizen_api.db   (43 MB)        │    │  쿼리 임베딩 생성               │
│  tizen_guide.db (19 MB)        │    │  코사인 유사도 검색             │
│           ↓                     │    │           ↓                     │
│  RPM 설치 ─────────────────────────→ /opt/usr/share/tizenclaw/rag/    │
└─────────────────────────────────┘    └─────────────────────────────────┘
```

## 동반 프로젝트: tizenclaw-assets

RAG 에셋은 통합 **[tizenclaw-assets](https://github.com/hjhun/tizenclaw-assets)** 패키지에 포함되며, ONNX Runtime, OCR 엔진, 임베딩 모델도 함께 제공됩니다.

| 컴포넌트 | 설치 경로 | 크기 |
|---------|----------|------|
| 지식 데이터베이스 | `/opt/usr/share/tizenclaw/rag/` | ~62 MB |
| ONNX Runtime | `/opt/usr/share/tizenclaw/lib/` | ~16 MB |
| 임베딩 모델 | `/opt/usr/share/tizenclaw/models/all-MiniLM-L6-v2/` | ~90 MB |
| OCR 모델 (PP-OCRv3) | `/opt/usr/share/tizenclaw/models/ppocr/` | ~15 MB (lite) / ~86 MB (full) |
| OCR CLI 도구 | `/opt/usr/share/tizenclaw/tools/cli/tizenclaw-ocr/` | ~130 KB |

### 지식 데이터베이스

| 데이터베이스 | 출처 | 파일 수 | 청크 수 |
|------------|------|-------:|-------:|
| `tizen_api.db` | 네이티브 C-API Doxygen (HTML) | 437 | 10,915 |
| `tizen_guide.db` | 네이티브 가이드 (Markdown) | 302 | 4,770 |

### 빌드 및 배포

```bash
# 방법 A: 자동 (deploy.sh 사용)
# deploy.sh가 ../tizenclaw-assets를 자동 탐지하여 함께 빌드
./deploy.sh

# 방법 B: 수동
cd ../tizenclaw-assets
gbs build -A x86_64 --include-all

# 방법 C: Full CJK OCR 모델 포함 (기본은 lite 한국어+영어)
cd ../tizenclaw-assets
gbs build -A x86_64 --include-all --define "ocr_model full"
```

### RAG 데이터베이스 재생성

소스 문서에서 지식 데이터베이스를 다시 빌드해야 하는 경우:

```bash
cd ../tizenclaw-assets

# GitHub에서 tizen-docs 자동 다운로드 (미존재 시)
./scripts/setup_docs.sh

# 로컬 임베딩으로 데이터베이스 빌드 (API 키 불필요)
pip3 install sentence-transformers
./scripts/build_knowledge_db.sh
```

## 온디바이스 임베딩 모듈

C++ 임베딩 모듈 (`src/tizenclaw/embedding/`)은 메인 tizenclaw 프로젝트에 포함:

| 파일 | 설명 |
|------|------|
| `wordpiece_tokenizer.{hh,cc}` | BERT 호환 WordPiece 토크나이저 |
| `on_device_embedding.{hh,cc}` | ONNX Runtime 추론 (dlopen), `all-MiniLM-L6-v2` |
| `onnxruntime_c_api.h` | 공식 ONNX Runtime C API 헤더 (v1.20.1) |

### 동작 원리

1. **토큰화** — WordPiece 어휘(`vocab.txt`, 30,522 토큰)로 입력 텍스트 토큰화
2. **추론** — ONNX Runtime이 `all-MiniLM-L6-v2` 모델 실행, 토큰별 hidden state 생성
3. **풀링** — attention mask를 사용한 mean pooling으로 384차원 임베딩 생성
4. **정규화** — 코사인 유사도 호환을 위한 L2 정규화
5. **검색** — SQLite 내 사전 계산된 임베딩에 대한 코사인 유사도 검색

### EmbeddingStore 멀티 DB 지원

```
embeddings.db (메인 저장소)
  ↓ ATTACH
knowledge_0 → tizen_guide.db
knowledge_1 → tizen_knowledge.db
knowledge_2 → tizen_api.db
```

쿼리는 모든 연결된 데이터베이스를 검색하여 상위 k개 가장 유사한 결과를 반환합니다.

## 지원 아키텍처

| 아키텍처 | ONNX Runtime | 상태 |
|:---:|:---:|:---:|
| x86_64 | ✅ 사전빌드 | 완전 지원 |
| aarch64 | ✅ 사전빌드 | 완전 지원 |
| armv7l | ✅ 크로스컴파일 | 완전 지원 |

armv7l 라이브러리는 `arm-linux-gnueabihf-gcc`를 사용하여 소스에서 크로스컴파일됩니다.
