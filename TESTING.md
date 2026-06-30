# Testing

```bash
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

The suite includes unit coverage for validation, authentication, message/tool conversion, arbitrary SSE byte boundaries, truncated streams, and request IDs. HTTP conformance tests use a mock OpenAI server to verify outbound requests and Anthropic JSON/SSE responses.

For a manual Claude Code smoke test:

```bash
cp config.example.yaml config.yaml
export OPENAI_API_KEY='...'
export PROXY_CLIENT_KEY='long-random-key'
cargo run --release -- serve --config config.yaml

ANTHROPIC_BASE_URL=http://localhost:8082 \
ANTHROPIC_API_KEY="$PROXY_CLIENT_KEY" \
claude
```

Prometheus metrics can be inspected at `http://127.0.0.1:9090/metrics` by default.
