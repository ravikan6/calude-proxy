# Implementation status

The original generated scaffold was replaced in version 0.2.0. The current implementation compiles as a library plus a thin CLI binary and includes:

- Validated YAML configuration and atomic SIGHUP reload
- Named client authentication, route authorization, and local limits
- Provider-neutral message domain and strict Anthropic validation
- OpenAI-compatible/Azure Chat Completions transport and translation
- Non-streaming and typed streaming response conversion
- Images, modern tools, parallel tool buffering, tool results, and usage
- Weighted priority routing, safe fallback, timeout handling, and circuits
- Target-tokenizer-backed token counting
- Anthropic-shaped errors, health probes, structured logging, and Prometheus metrics
- Unit and mock-upstream HTTP conformance tests

Native Anthropic, OpenAI Responses, distributed state, semantic caching, and non-portable Anthropic features are deliberately outside v1.
