# TizenClaw C-API Guide (`libtizenclaw`)

The `libtizenclaw` library provides a native C-API for interacting with the TizenClaw daemon from other Tizen applications. It encapsulates the underlying IPC mechanisms (JSON-RPC 2.0 over Abstract Unix Domain Sockets) and provides a simple, asynchronous, callback-driven interface.

## Prerequisites

To use `libtizenclaw` in your Tizen application, include the header file:

```c
#include <tizenclaw.h>
```

Ensure your application links against `libtizenclaw.so`.

## Initialization and Cleanup

Before sending any requests, you must create a TizenClaw client handle. When you are finished, you must destroy it to free associated resources.

### 1. Create a Client

**API:** `int tizenclaw_client_create(tizenclaw_client_h *client);`

Initializes a new client handle.

```c
tizenclaw_client_h client = NULL;
int ret = tizenclaw_client_create(&client);
if (ret != TIZENCLAW_ERROR_NONE) {
    // Handle error (e.g., TIZENCLAW_ERROR_OUT_OF_MEMORY)
}
```

### 2. Destroy a Client

**API:** `int tizenclaw_client_destroy(tizenclaw_client_h client);`

Cleans up the client handle and disconnects from the daemon if connected.

```c
tizenclaw_client_destroy(client);
client = NULL;
```

## Sending Requests

TizenClaw provides two primary methods for sending prompts to the agent daemon: **Standard Asynchronous** and **Streaming**. 

### 1. Standard Asynchronous Request

Use this method when you want to receive the entire response at once after the agent has finished processing.

**API:**
```c
int tizenclaw_client_send_request(tizenclaw_client_h client, 
                                  const char *session_id, 
                                  const char *prompt,
                                  tizenclaw_response_cb response_cb, 
                                  tizenclaw_error_cb error_cb, 
                                  void *user_data);
```

- `session_id`: An optional string to maintain conversation history. Pass `NULL` or an empty string `""` to use the default session.
- `prompt`: The text instruction for the LLM agent.
- `response_cb`: Callback invoked when the final response is received.
- `error_cb`: Callback invoked if an error occurs during execution.
- `user_data`: Custom pointer passed back to your callbacks.

#### Example:

```c
void on_response(const char *session_id, const char *response, void *user_data) {
    printf("Response for session %s: %s\n", session_id ? session_id : "default", response);
}

void on_error(const char *session_id, int error_code, const char *error_message, void *user_data) {
    printf("Error [%d]: %s\n", error_code, error_message);
}

// Sending the prompt
int ret = tizenclaw_client_send_request(client, "my_session", "Hello, TizenClaw!", 
                                        on_response, on_error, NULL);
if (ret != TIZENCLAW_ERROR_NONE) {
    printf("Failed to send request.\n");
}
```

### 2. Streaming Request

Use this method when you want to receive the response progressively as the LLM generates tokens. This is ideal for chat interfaces or to show progress to the user.

**Why use Streaming?**
Generating a complete response from a Large Language Model (LLM) takes time. If a user asks a complex question, the Standard Asynchronous API might appear unresponsive for several seconds while the text is being generated. The Streaming API solves this by returning text "chunks" as soon as they are inferred, allowing you to create a real-time "typing" effect on the UI, thus vastly improving the user experience (UX).

**Continuing a Conversation (Session ID)**
It is important to note that maintaining the conversation context is independent of whether you use the Standard or Streaming API. To continue a dialogue, simply provide the exact same `session_id` continuously. The TizenClaw daemon manages the history based strictly on this ID.

**API:**
```c
int tizenclaw_client_send_request_stream(tizenclaw_client_h client, 
                                         const char *session_id, 
                                         const char *prompt,
                                         tizenclaw_stream_cb stream_cb, 
                                         tizenclaw_error_cb error_cb, 
                                         void *user_data);
```

- `stream_cb`: Callback invoked continuously as text chunks are received. The `is_done` boolean flag indicates the final chunk.

#### Example:

```c
void on_stream_chunk(const char *session_id, const char *chunk, bool is_done, void *user_data) {
    printf("%s", chunk); 
    fflush(stdout);
    
    if (is_done) {
        printf("\n[Stream completed]\n");
    }
}

// Sending the streaming prompt
int ret = tizenclaw_client_send_request_stream(client, NULL, "Write a short poem about Tizen.", 
                                               on_stream_chunk, on_error, NULL);
```

## Error Codes

The API returns standard TizenClaw error codes defined in `<tizenclaw_error.h>`, including:

| Code | Description |
|------|-------------|
| `TIZENCLAW_ERROR_NONE` | Operation successful (0) |
| `TIZENCLAW_ERROR_INVALID_PARAMETER` | Null pointers or invalid arguments provided |
| `TIZENCLAW_ERROR_OUT_OF_MEMORY` | Memory allocation failed during handle creation |
| `TIZENCLAW_ERROR_CONNECTION_REFUSED` | Daemon is either not running or the socket file is inaccessible |

## Best Practices

1. **Callback Thread Safety**: Callbacks (`response_cb`, `stream_cb`, `error_cb`) execute on an internal glib worker thread managed by the library. If you are updating a UI (like EFL/Ecore), ensure you marshal the data to the main UI thread (e.g., using `ecore_main_loop_thread_safe_call_async`).
2. **Session IDs**: Use unique `session_id` identifiers for distinct conversations or agents to prevent context leakage between different users or tasks.
3. **Lifecycles**: A single `tizenclaw_client_h` can be reused for multiple sequential or concurrent requests. DO NOT create and destroy the client for every single prompt.
