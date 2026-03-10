# TizenClaw C-API 가이드 (`libtizenclaw`)

`libtizenclaw` 라이브러리는 외부 Tizen 애플리케이션에서 TizenClaw 데몬과 상호작용할 수 있도록 네이티브 C-API를 제공합니다. 시스템 내부의 IPC 메커니즘(Abstract Unix Domain Sockets 기반 JSON-RPC 2.0)을 캡슐화하여, 비동기 콜백 기반의 단순한 인터페이스를 노출합니다.

## 사전 준비 (Prerequisites)

Tizen 애플리케이션에서 `libtizenclaw`를 사용하려면 다음 헤더를 포함하십시오:

```c
#include <tizenclaw.h>
```

프로젝트 빌드 시 `libtizenclaw.so` 라이브러리와 링크되도록 설정해야 합니다.

## 초기화 및 정리 (Initialization and Cleanup)

데몬에 요청을 보내기 전 반드시 TizenClaw 클라이언트 핸들을 생성해야 하며, 사용이 끝난 후에는 관련 리소스를 해제하기 위해 파괴(destroy)해야 합니다.

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

### 2. 클라이언트 정리

**API:** `int tizenclaw_client_destroy(tizenclaw_client_h client);`

클라이언트 핸들을 정리하고, 연결되어 있다면 데몬과의 연결을 해제합니다.

```c
tizenclaw_client_destroy(client);
client = NULL;
```

## 요청 전송 (Sending Requests)

TizenClaw는 에이전트 데몬에 프롬프트를 전송하기 위한 두 가지 주요 방식을 제공합니다: **표준 비동기(Standard Asynchronous)** 방식과 **스트리밍(Streaming)** 방식입니다.

### 1. 표준 비동기 요청 (Standard Asynchronous Request)

에이전트가 처리를 모두 마친 뒤 응답 전체를 한 번에 받고자 할 때 사용합니다.

**API:**
```c
int tizenclaw_client_send_request(tizenclaw_client_h client, 
                                  const char *session_id, 
                                  const char *prompt,
                                  tizenclaw_response_cb response_cb, 
                                  tizenclaw_error_cb error_cb, 
                                  void *user_data);
```

- `session_id`: 대화 내역(History)을 유지하기 위한 선택적 식별자 문자열입니다. `NULL`이나 빈 문자열 `""`을 전달하면 기본 세션을 사용합니다.
- `prompt`: LLM 에이전트에게 지시할 텍스트입니다.
- `response_cb`: 텍스트 응답이 최종적으로 완성되었을 때 호출되는 콜백입니다.
- `error_cb`: 실행 중 에러 발생 시 호출되는 콜백입니다.
- `user_data`: 콜백으로 다시 돌려받을 커스텀 포인터입니다.

#### 예제:

```c
void on_response(const char *session_id, const char *response, void *user_data) {
    printf("Session %s 응답: %s\n", session_id ? session_id : "default", response);
}

void on_error(const char *session_id, int error_code, const char *error_message, void *user_data) {
    printf("에러 발생 [%d]: %s\n", error_code, error_message);
}

// 프롬프트 전송
int ret = tizenclaw_client_send_request(client, "my_session", "Hello, TizenClaw!", 
                                        on_response, on_error, NULL);
if (ret != TIZENCLAW_ERROR_NONE) {
    printf("요청 전송 실패.\n");
}
```

### 2. 스트리밍 요청 (Streaming Request)

LLM이 토큰을 생성하는 즉시 실시간으로 응답을 받아야 할 때 사용합니다. 채팅 UI 구현이나 사용자에게 진행 상황을 보여주는 데 이상적입니다.

**스트리밍 API는 왜 필요한가요?**
대규모 언어 모델(LLM)이 답변 전체를 완성하는 데에는 수 초 이상의 시간이 걸릴 수 있습니다. 일반 비동기 API 방식은 전체 텍스트가 완성될 때까지 콜백이 오지 않아, 사용자는 앱이 멈춘 것으로 오해하거나 지루함을 느낄 수 있습니다. 반면, 스트리밍 API를 사용하면 타자를 치듯 단어가 생성되는 즉시 작은 조각(chunk) 단위로 콜백이 들어오기 때문에 즉각적이고 부드러운 사용자 경험(UX)을 제공할 수 있습니다.

**대화 흐름 이어가기 (Session ID)**
이전 대화의 문맥을 기억하고 이어나가는 것(Context 유지)은 방식에 상관없이 완전히 동일한 원리가 적용됩니다. TizenClaw 데몬은 전달받은 `session_id`를 기준으로 대화 기록을 관리하므로, 어떤 API를 호출하든 **동일한 `session_id` 문자열**만 계속해서 유지해주시면 됩니다.

**API:**
```c
int tizenclaw_client_send_request_stream(tizenclaw_client_h client, 
                                         const char *session_id, 
                                         const char *prompt,
                                         tizenclaw_stream_cb stream_cb, 
                                         tizenclaw_error_cb error_cb, 
                                         void *user_data);
```

- `stream_cb`: 텍스트 청크가 생성될 때마다 연속적으로 호출되는 콜백입니다. `is_done` 불리언 값을 통해 마지막 청크인지 여부를 판별합니다.

#### 예제:

```c
void on_stream_chunk(const char *session_id, const char *chunk, bool is_done, void *user_data) {
    printf("%s", chunk); 
    fflush(stdout);
    
    if (is_done) {
        printf("\n[스트리밍 완료]\n");
    }
}

// 스트리밍 프롬프트 전송
int ret = tizenclaw_client_send_request_stream(client, NULL, "타이젠에 대한 짧은 시를 써줘.", 
                                               on_stream_chunk, on_error, NULL);
```

## 에러 코드 (Error Codes)

API는 `<tizenclaw_error.h>`에 정의된 표준 TizenClaw 에러 코드를 반환합니다.

| 분류 | 설명 |
|------|-------------|
| `TIZENCLAW_ERROR_NONE` | 성공적으로 처리됨 (0) |
| `TIZENCLAW_ERROR_INVALID_PARAMETER` | 전달된 매개변수가 형식에 맞지 않거나 NULL 포인터가 입력됨 |
| `TIZENCLAW_ERROR_OUT_OF_MEMORY` | 객체 생성 및 메모리 할당 중 실패 (OOM) |
| `TIZENCLAW_ERROR_CONNECTION_REFUSED` | 데몬이 실행 중이 아니거나 소켓 파일 권한 등으로 접근할 수 없음 |

## 모범 사례 (Best Practices)

1. **콜백 스레드 안전성 (Thread Safety)**: 모든 콜백(`response_cb`, `stream_cb`, `error_cb`)은 라이브러리가 관리하는 내부 glib 워커 스레드 위에서 실행됩니다. EFL/Ecore 같은 UI 프레임워크를 조작해야 할 경우 반드시 메인 UI 스레드로 컨텍스트를 넘겨서 동기화 후 처리해야 합니다(예: `ecore_main_loop_thread_safe_call_async` 사용).
2. **세션 식별자**: 서로 다른 대화 컨텍스트나 독립적인 사용자 태스크마다 고유한 `session_id`를 부여하세요. 이를 통해 히스토리 정보가 교차 간섭받지 않습니다.
3. **생명주기 유지**: 하나의 `tizenclaw_client_h` 인스턴스로 복수의 요청이나 동시 비동기 요청을 여러 개 처리할 수 있습니다. 각 요청(프롬프트)마다 클라이언트 핸들을 생성하고 파괴하는 것을 피해주십시오.
