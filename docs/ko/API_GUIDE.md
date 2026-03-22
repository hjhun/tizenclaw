# TizenClaw C-API 가이드 (`libtizenclaw`)

> **최종 업데이트**: 2026-03-22

`libtizenclaw` 라이브러리는 다른 Tizen 앱에서 TizenClaw 데몬과 통신하기 위한 네이티브 C-API를 제공합니다. 내부 IPC 메커니즘(Abstract Unix Domain Socket을 통한 JSON-RPC 2.0)을 캡슐화하여 간단하고 비동기적인 콜백 기반 인터페이스를 제공합니다.

## 사전 요구사항

앱에서 `libtizenclaw`를 사용하려면 헤더를 포함하세요:

```c
#include <tizenclaw.h>
```

`libtizenclaw.so`에 대한 링크를 확인하세요.

## 초기화 및 정리

요청을 보내기 전에 클라이언트 핸들을 생성해야 합니다. 작업이 끝나면 반드시 소멸시켜 리소스를 해제하세요.

### 1. 클라이언트 생성

**API:** `int tizenclaw_client_create(tizenclaw_client_h *client);`

새 클라이언트 핸들을 초기화합니다.

```c
tizenclaw_client_h client = NULL;
int ret = tizenclaw_client_create(&client);
if (ret != TIZENCLAW_ERROR_NONE) {
    // 에러 처리 (예: TIZENCLAW_ERROR_OUT_OF_MEMORY)
}
```

### 2. 클라이언트 소멸

**API:** `int tizenclaw_client_destroy(tizenclaw_client_h client);`

클라이언트 핸들을 정리하고, 연결 중이면 데몬과 연결을 해제합니다.

```c
tizenclaw_client_destroy(client);
client = NULL;
```

## 요청 전송

TizenClaw는 에이전트 데몬에 프롬프트를 전송하는 두 가지 주요 방법을 제공합니다: **표준 비동기** 및 **스트리밍**.

### 1. 표준 비동기 요청

에이전트가 처리를 완료한 후 전체 응답을 한 번에 받고 싶을 때 사용합니다.

**API:**
```c
int tizenclaw_client_send_request(tizenclaw_client_h client, 
                                  const char *session_id, 
                                  const char *prompt,
                                  tizenclaw_response_cb response_cb, 
                                  tizenclaw_error_cb error_cb, 
                                  void *user_data);
```

- `session_id`: 대화 히스토리를 유지하기 위한 선택적 문자열. `NULL` 또는 빈 문자열 `""`을 전달하면 기본 세션 사용.
- `prompt`: LLM 에이전트에 보내는 텍스트 지시.
- `response_cb`: 최종 응답 수신 시 호출되는 콜백.
- `error_cb`: 실행 중 에러 발생 시 호출되는 콜백.
- `user_data`: 콜백에 전달되는 사용자 정의 포인터.

#### 예제:

```c
void on_response(const char *session_id, const char *response, void *user_data) {
    printf("세션 %s 응답: %s\n", session_id ? session_id : "default", response);
}

void on_error(const char *session_id, int error_code, const char *error_message, void *user_data) {
    printf("에러 [%d]: %s\n", error_code, error_message);
}

// 프롬프트 전송
int ret = tizenclaw_client_send_request(client, "my_session", "안녕, TizenClaw!", 
                                        on_response, on_error, NULL);
if (ret != TIZENCLAW_ERROR_NONE) {
    printf("요청 전송 실패.\n");
}
```

### 2. 스트리밍 요청

LLM이 토큰을 생성하는 대로 응답을 점진적으로 받고 싶을 때 사용합니다. 채팅 인터페이스나 사용자에게 진행 상황을 보여줄 때 이상적입니다.

**왜 스트리밍을 사용하는가?**
LLM에서 완전한 응답을 생성하는 데는 시간이 걸립니다. 복잡한 질문의 경우 표준 비동기 API는 텍스트 생성 중 수 초간 무응답으로 보일 수 있습니다. 스트리밍 API는 추론되는 즉시 텍스트 "청크"를 반환하여, UI에서 실시간 "타이핑" 효과를 만들 수 있으므로 UX를 크게 개선합니다.

**대화 연속 (세션 ID)**
대화 컨텍스트 유지는 표준/스트리밍 API와 무관합니다. 대화를 연속하려면 동일한 `session_id`를 계속 제공하세요. TizenClaw 데몬이 이 ID를 기반으로 히스토리를 관리합니다.

**API:**
```c
int tizenclaw_client_send_request_stream(tizenclaw_client_h client, 
                                         const char *session_id, 
                                         const char *prompt,
                                         tizenclaw_stream_cb stream_cb, 
                                         tizenclaw_error_cb error_cb, 
                                         void *user_data);
```

- `stream_cb`: 텍스트 청크 수신 시 계속 호출되는 콜백. `is_done` boolean 플래그가 마지막 청크를 나타냄.

#### 예제:

```c
void on_stream_chunk(const char *session_id, const char *chunk, bool is_done, void *user_data) {
    printf("%s", chunk); 
    fflush(stdout);
    
    if (is_done) {
        printf("\n[스트림 완료]\n");
    }
}

// 스트리밍 프롬프트 전송
int ret = tizenclaw_client_send_request_stream(client, NULL, "Tizen에 대한 짧은 시를 써줘.", 
                                               on_stream_chunk, on_error, NULL);
```

## 에러 코드

API는 `<tizenclaw_error.h>`에 정의된 표준 TizenClaw 에러 코드를 반환합니다:

| 코드 | 설명 |
|------|------|
| `TIZENCLAW_ERROR_NONE` | 작업 성공 (0) |
| `TIZENCLAW_ERROR_INVALID_PARAMETER` | NULL 포인터 또는 잘못된 인자 |
| `TIZENCLAW_ERROR_OUT_OF_MEMORY` | 핸들 생성 중 메모리 할당 실패 |
| `TIZENCLAW_ERROR_CONNECTION_REFUSED` | 데몬이 실행 중이 아니거나 소켓 파일에 접근 불가 |

## 모범 사례

1. **콜백 스레드 안전성**: 콜백 (`response_cb`, `stream_cb`, `error_cb`)은 라이브러리가 관리하는 내부 glib 워커 스레드에서 실행됩니다. UI(예: EFL/Ecore)를 업데이트하는 경우 메인 UI 스레드로 데이터를 마샬링하세요 (예: `ecore_main_loop_thread_safe_call_async`).
2. **세션 ID**: 서로 다른 대화나 에이전트에 고유한 `session_id`를 사용하여 컨텍스트 누출을 방지하세요.
3. **라이프사이클**: 하나의 `tizenclaw_client_h`를 여러 순차/동시 요청에 재사용할 수 있습니다. 매 프롬프트마다 클라이언트를 생성/소멸하지 마세요.
