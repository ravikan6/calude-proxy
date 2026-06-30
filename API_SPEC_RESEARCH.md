# Latest LLM API Specifications Research

## Claude API (Anthropic) - Latest Spec

### Endpoint: `/v1/messages`

#### Request Format:
```json
{
  "model": "claude-3-5-sonnet-20241022",
  "max_tokens": 1024,
  "messages": [
    {
      "role": "user",
      "content": "Hello, Claude!"
    }
  ],
  "stream": false,
  "temperature": 1.0,
  "top_p": 1.0,
  "top_k": 0,
  "stop_sequences": [],
  "metadata": {},
  "system": "You are a helpful assistant.",
  "tools": [],
  "tool_choice": {}
}
```

#### Response Format:
```json
{
  "id": "msg_013Z1...",
  "type": "message",
  "role": "assistant",
  "model": "claude-3-5-sonnet-20241022",
  "content": [
    {
      "type": "text",
      "text": "Hello! How can I help you?"
    }
  ],
  "stop_reason": "end_turn",
  "stop_sequence": null,
  "usage": {
    "input_tokens": 10,
    "output_tokens": 20
  }
}
```

#### Streaming Response (SSE):
```
event: message_start
data: {"type":"message_start","message":{"id":"msg_013Z1...","type":"message","role":"assistant","model":"claude-3-5-sonnet-20241022","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: ping
data: {"type":"ping"}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":5}}

event: message_stop
data: {"type":"message_stop"}
```

## OpenAI API - Latest Spec (2026)

### Endpoint: `/v1/chat/completions`

#### Request Format:
```json
{
  "model": "gpt-4o",
  "messages": [
    {
      "role": "user",
      "content": "Hello!"
    }
  ],
  "temperature": 1.0,
  "max_tokens": 1024,
  "top_p": 1.0,
  "n": 1,
  "stream": false,
  "stop": null,
  "presence_penalty": 0,
  "frequency_penalty": 0,
  "logit_bias": {},
  "user": "user-1234",
  "functions": [],
  "function_call": "auto",
  "response_format": {}
}
```

#### Response Format:
```json
{
  "id": "chatcmpl-123",
  "object": "chat.completion",
  "created": 1677665662,
  "model": "gpt-4o",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Hello! How can I help?"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 10,
    "completion_tokens": 20,
    "total_tokens": 30
  }
}
```

#### Streaming Response:
```
data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1677665662,"model":"gpt-4o","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1677665662,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1677665662,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":"!"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1677665662,"model":"gpt-4o","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]
```

## Key Differences

### 1. Message Format
- **Claude**: Content is an array of blocks (text, images, tool_use, tool_result)
- **OpenAI**: Content is a simple string (or array for multimodal)

### 2. Role Names
- **Claude**: `user`, `assistant`, `system`, `tool`
- **OpenAI**: `user`, `assistant`, `system`, `function`

### 3. Tool/Function Calling
- **Claude**: `tool_use` content blocks with `input` field
- **OpenAI**: `function_call` in message with `arguments` string

### 4. Streaming Format
- **Claude**: SSE events with specific types (message_start, content_block_delta, etc.)
- **OpenAI**: Chunked JSON responses with `delta` field

### 5. Stop Reasons
- **Claude**: `end_turn`, `max_tokens`, `tool_use`, `stop_sequence`
- **OpenAI**: `stop`, `length`, `function_call`, `content_filter`, `tool_calls`

## Conversion Requirements

### Claude → OpenAI

1. **Model Mapping**:
   - `claude-3-5-sonnet-*` → `gpt-4o` (or configured MIDDLE_MODEL)
   - `claude-3-haiku-*` → `gpt-4o-mini` (or configured SMALL_MODEL)
   - `claude-3-opus-*` → `gpt-4o` (or configured BIG_MODEL)

2. **Messages Conversion**:
   - System message → OpenAI system role
   - User messages → OpenAI user role
   - Assistant messages → OpenAI assistant role
   - Tool results → OpenAI function role

3. **Content Conversion**:
   - Text blocks → Simple string content
   - Image blocks → OpenAI image_url format
   - Tool use → Function call with JSON arguments
   - Tool results → Function response

4. **Parameters**:
   - `max_tokens` → `max_tokens`
   - `temperature` → `temperature`
   - `top_p` → `top_p`
   - `top_k` → (ignore, OpenAI doesn't support)
   - `stop_sequences` → `stop`

### OpenAI → Claude

1. **Response Structure**:
   - Single choice → Claude message format
   - Text content → Text content block
   - Function calls → Tool use content blocks

2. **Streaming Conversion**:
   - OpenAI chunks → Claude SSE events
   - Delta content → content_block_delta events
   - Function call chunks → tool_use content blocks

3. **Finish Reasons**:
   - `stop` → `end_turn`
   - `length` → `max_tokens`
   - `function_call` → `tool_use`
   - `tool_calls` → `tool_use`

## Implementation Plan

### 1. Request Conversion (Claude → OpenAI)

```rust
fn convert_claude_to_openai(request: ClaudeRequest) -> OpenAIRequest {
    // Map model
    // Convert messages
    // Convert tools to functions
    // Map parameters
}
```

### 2. Response Conversion (OpenAI → Claude)

```rust
fn convert_openai_to_claude(response: OpenAIResponse, original_request: ClaudeRequest) -> ClaudeResponse {
    // Map response structure
    // Convert content blocks
    // Map finish reasons
    // Calculate token usage
}
```

### 3. Streaming Conversion

```rust
async fn convert_streaming(openai_stream: OpenAIStream, original_request: ClaudeRequest) -> ClaudeStream {
    // Convert SSE format
    // Handle incremental deltas
    // Manage tool call state
    // Emit proper Claude events
}
```

## References

- Claude API Docs: https://docs.anthropic.com/claude/docs
- OpenAI API Docs: https://platform.openai.com/docs/api-reference
- OpenAI Cookbook: https://github.com/openai/openai-cookbook
