use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use claude_code_proxy::{build_app, AppConfig, Runtime};
use http_body_util::BodyExt;
use mockito::{Matcher, Server};
use serde_json::{json, Value};
use tower::ServiceExt;

fn config(endpoint: &str) -> AppConfig {
    std::env::set_var("CONFORMANCE_CLIENT_KEY", "client-secret-value-123456");
    std::env::set_var("CONFORMANCE_PROVIDER_KEY", "provider-secret-value");
    serde_yaml::from_str(&format!(
        r#"
server:
  bind: "127.0.0.1:0"
  metrics_bind: null
clients:
  - id: test-client
    key: {{ env: CONFORMANCE_CLIENT_KEY }}
    allowed_routes: [default]
providers:
  - id: mock
    kind: openai_chat
    endpoint: "{endpoint}"
    allow_insecure_http: true
    credential:
      type: bearer
      secret: {{ env: CONFORMANCE_PROVIDER_KEY }}
routes:
  - id: default
    models: ["claude-*"]
    targets:
      - provider: mock
        model: upstream-model
"#
    ))
    .unwrap()
}

fn request(body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .header("anthropic-version", "2023-06-01")
        .header("x-api-key", "client-secret-value-123456")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn translates_non_streaming_messages_and_tools() {
    let mut server = Server::new_async().await;
    let mock = server.mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer provider-secret-value")
        .match_body(Matcher::PartialJson(json!({
            "model":"upstream-model",
            "messages":[{"role":"user","content":"use the tool"}],
            "tools":[{"type":"function","function":{"name":"lookup","description":null,"parameters":{"type":"object"}}}]
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({
            "id":"chatcmpl_1",
            "choices":[{"message":{"content":null,"tool_calls":[{"id":"call_1","function":{"name":"lookup","arguments":"{\"query\":\"rust\"}"}}]},"finish_reason":"tool_calls"}],
            "usage":{"prompt_tokens":12,"completion_tokens":4}
        }).to_string())
        .create_async().await;

    let runtime = Runtime::new(config(&server.url())).unwrap();
    let app = build_app(Arc::new(ArcSwap::from_pointee(runtime)));
    let response = app.oneshot(request(json!({
        "model":"claude-sonnet-test","max_tokens":100,"messages":[{"role":"user","content":"use the tool"}],
        "tools":[{"name":"lookup","input_schema":{"type":"object"}}]
    }))).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("request-id"));
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["model"], "claude-sonnet-test");
    assert_eq!(body["content"][0]["type"], "tool_use");
    assert_eq!(body["content"][0]["input"]["query"], "rust");
    assert_eq!(body["stop_reason"], "tool_use");
    assert_eq!(body["usage"]["input_tokens"], 12);
    mock.assert_async().await;
}

#[tokio::test]
async fn emits_anthropic_stream_lifecycle_and_final_usage() {
    let mut server = Server::new_async().await;
    let stream = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}],\"usage\":null}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":null}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":2}}\n\n",
        "data: [DONE]\n\n"
    );
    let mock = server
        .mock("POST", "/chat/completions")
        .match_body(Matcher::PartialJson(
            json!({"stream":true,"stream_options":{"include_usage":true}}),
        ))
        .with_status(200)
        .with_header("content-type", "text/event-stream")
        .with_body(stream)
        .create_async()
        .await;

    let runtime = Runtime::new(config(&server.url())).unwrap();
    let app = build_app(Arc::new(ArcSwap::from_pointee(runtime)));
    let response = app
        .oneshot(request(json!({
            "model":"claude-sonnet-test","max_tokens":100,"stream":true,
            "messages":[{"role":"user","content":"hello"}]
        })))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let body = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    let start = body.find("event: message_start").unwrap();
    let block = body.find("event: content_block_start").unwrap();
    let delta = body.find("event: content_block_delta").unwrap();
    let message_delta = body.find("event: message_delta").unwrap();
    let stop = body.find("event: message_stop").unwrap();
    assert!(start < block && block < delta && delta < message_delta && message_delta < stop);
    assert!(body.contains("\"output_tokens\":2"));
    mock.assert_async().await;
}

#[tokio::test]
async fn uses_anthropic_error_shape_for_authentication() {
    let server = Server::new_async().await;
    let runtime = Runtime::new(config(&server.url())).unwrap();
    let app = build_app(Arc::new(ArcSwap::from_pointee(runtime)));
    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .header("anthropic-version", "2023-06-01")
        .body(Body::from(
            json!({"model":"claude-x","max_tokens":1,"messages":[{"role":"user","content":"x"}]})
                .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["type"], "error");
    assert_eq!(body["error"]["type"], "authentication_error");
    assert!(body["request_id"].as_str().unwrap().starts_with("req_"));
}

#[tokio::test]
async fn falls_back_on_retryable_upstream_status() {
    let mut primary = Server::new_async().await;
    let primary_mock = primary
        .mock("POST", "/chat/completions")
        .with_status(503)
        .with_body("overloaded")
        .create_async()
        .await;
    let mut fallback = Server::new_async().await;
    let fallback_mock = fallback.mock("POST", "/chat/completions")
        .with_status(200).with_header("content-type", "application/json")
        .with_body(json!({
            "id":"fallback", "choices":[{"message":{"content":"ok","tool_calls":[]},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":2,"completion_tokens":1}
        }).to_string()).create_async().await;
    std::env::set_var("CONFORMANCE_CLIENT_KEY", "client-secret-value-123456");
    std::env::set_var("CONFORMANCE_PROVIDER_KEY", "provider-secret-value");
    let config: AppConfig = serde_yaml::from_str(&format!(
        r#"
server: {{ bind: "127.0.0.1:0", metrics_bind: null }}
limits: {{ max_attempts: 2 }}
clients:
  - id: test-client
    key: {{ env: CONFORMANCE_CLIENT_KEY }}
providers:
  - id: primary
    kind: openai_chat
    endpoint: "{}"
    allow_insecure_http: true
    credential: {{ type: bearer, secret: {{ env: CONFORMANCE_PROVIDER_KEY }} }}
  - id: fallback
    kind: openai_chat
    endpoint: "{}"
    allow_insecure_http: true
    credential: {{ type: bearer, secret: {{ env: CONFORMANCE_PROVIDER_KEY }} }}
routes:
  - id: default
    models: ["claude-*"]
    targets:
      - {{ provider: primary, model: first, priority: 1 }}
      - {{ provider: fallback, model: second, priority: 2 }}
"#,
        primary.url(),
        fallback.url()
    ))
    .unwrap();
    let app = build_app(Arc::new(ArcSwap::from_pointee(
        Runtime::new(config).unwrap(),
    )));
    let response = app
        .oneshot(request(json!({
            "model":"claude-test","max_tokens":10,"messages":[{"role":"user","content":"x"}]
        })))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["id"], "fallback");
    primary_mock.assert_async().await;
    fallback_mock.assert_async().await;
}

#[tokio::test]
async fn counts_target_tokens_without_calling_upstream() {
    let server = Server::new_async().await;
    let runtime = Runtime::new(config(&server.url())).unwrap();
    let app = build_app(Arc::new(ArcSwap::from_pointee(runtime)));
    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages/count_tokens")
        .header("content-type", "application/json")
        .header("anthropic-version", "2023-06-01")
        .header("x-api-key", "client-secret-value-123456")
        .body(Body::from(
            json!({
                "model":"claude-test","messages":[{"role":"user","content":"count these tokens"}],
                "tools":[{"name":"lookup","input_schema":{"type":"object"}}]
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(body["input_tokens"].as_u64().unwrap() > 5);
}
