// Integration tests for configuration versioning and rollback functionality

use claude_code_proxy::{
    build_dashboard_routes, create_db_pool, create_initial_admin_user,
    create_sample_config, create_sample_client, create_sample_provider,
    create_sample_route, initialize_database, DashboardState,
};
use axum::{body::Body, http::Request, Router};
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::Arc;
use tower::ServiceExt;

#[sqlx::test]
async fn test_config_versioning(pool: SqlitePool) -> sqlx::Result<()> {
    // Initialize database and sample data
    initialize_database(&pool).await?;
    create_sample_config(&pool).await?;
    create_sample_client(&pool).await?;
    create_sample_provider(&pool).await?;
    create_sample_route(&pool).await?;
    create_initial_admin_user(&pool, "admin-secret").await?;

    // Create dashboard state
    let state = Arc::new(DashboardState {
        db_pool: pool.clone(),
        admin_secret: "admin-secret".to_string(),
    });

    // Build the router
    let app = build_dashboard_routes(state.clone());

    // Test creating a new version
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/config")
                .header("Content-Type", "application/json")
                .body(Body::from(json!({
                    "server": {
                        "bind": "0.0.0.0:8080",
                        "metrics_bind": "0.0.0.0:8081",
                        "shutdown_grace_seconds": 30,
                    },
                    "limits": {
                        "max_body_bytes": 1048576,
                        "max_headers_bytes": 8192,
                    },
                    "clients": [],
                    "providers": [],
                    "routes": [],
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Test listing versions
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/versions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Test reverting to a version
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/versions/1/revert")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    Ok(())
}