# Rust proxy architecture

## Data path

`server` authenticates and bounds each request. `anthropic` validates it into provider-neutral domain types. `routing` selects only targets whose declared capabilities preserve the request. `provider` translates to modern OpenAI Chat Completions, owns pooled HTTP transport, and decodes responses into typed events. Anthropic JSON or SSE is encoded only at the edge.

The split prevents provider wire types from leaking into routing or client protocol code.

## Failure semantics

- Validation and capability failures return 400 and never fall back.
- Client authentication and authorization failures return 401/403.
- Connect failures, 408, 429, 5xx, and 529 may move to the next configured target before client bytes are emitted.
- Ambiguous failures after streaming starts never fall back; an Anthropic `error` SSE event is emitted.
- Circuit breakers are local to an instance. Cluster-wide quotas and health coordination belong at the deployment ingress.
- Racing and hedging are intentionally disabled because they duplicate model work and cost.

## Provider endpoints

`endpoint` is the URL immediately above `/chat/completions`:

- OpenAI: `https://api.openai.com/v1`
- Ollama: `http://ollama:11434/v1` with `allow_insecure_http: true`
- Azure unified v1: `https://RESOURCE.openai.azure.com/openai/v1`
- Azure legacy deployment: include `/openai/deployments/DEPLOYMENT` and any required API-version query in an ingress rewrite; unified v1 is preferred.

Capability profiles are explicit because OpenAI-compatible models differ. Configure tokenizer, vision/tools/parallel-tools, sampling and stop support, maximum output, and whether the model requires `max_completion_tokens`.

## Reload and shutdown

SIGHUP builds and validates a complete replacement runtime, including secrets and provider clients, before swapping it in. In-flight requests retain the old runtime. Listener and body-limit settings are immutable and require restart.

SIGINT/SIGTERM stops admission and lets Axum drain active requests. Dropping a client stream drops the corresponding upstream response stream and releases concurrency permits.

## Security defaults

- HTTPS provider endpoints unless insecure HTTP is explicitly enabled.
- No request-controlled upstream URLs.
- Constant-time hashes for client-key comparison.
- No prompt, tool payload, image, or secret logging.
- CORS disabled.
- Credential, host, content-length, and hop-by-hop custom headers forbidden.
- Metrics use bounded configured identifiers rather than user/model strings.
