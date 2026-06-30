FROM rust:1.86-bookworm AS builder
WORKDIR /build
COPY rust-proxy/Cargo.toml rust-proxy/Cargo.lock ./
COPY rust-proxy/src ./src
RUN cargo build --locked --release

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --no-create-home --shell /usr/sbin/nologin llmproxy
COPY --from=builder /build/target/release/claude-code-proxy /usr/local/bin/claude-code-proxy
USER 10001:10001
EXPOSE 8082 9090
ENTRYPOINT ["claude-code-proxy"]
CMD ["serve", "--config", "/etc/claude-code-proxy/config.yaml"]
