use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use crate::{
    config::{AppConfig, ClientConfig, ProviderConfig, RouteConfig},
    error::{ErrorKind, ProxyError, Result},
};

pub struct DashboardState {
    pub db_pool: SqlitePool,
    pub admin_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(error: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
        }
    }
}

impl<T> IntoResponse for ApiResponse<T>
where
    T: Serialize,
{
    fn into_response(self) -> axum::response::Response {
        Json(self).into_response()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigVersion {
    pub id: i64,
    pub version: i32,
    pub config_data: String,
    pub created_at: String,
    pub created_by: String,
}

pub async fn initialize_database(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!().run(pool).await.map_err(|e| {
        ProxyError::new(
            ErrorKind::Internal,
            format!("Failed to run database migrations: {}", e),
        )
    })?;
    Ok(())
}

pub async fn create_db_pool(database_url: &str) -> Result<SqlitePool> {
    SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .map_err(|e| {
            ProxyError::new(
                ErrorKind::Internal,
                format!("Failed to create database pool: {}", e),
            )
        })
}

pub fn build_dashboard_routes(state: Arc<DashboardState>) -> Router {
    Router::new()
        .route("/api/config", get(get_config))
        .route("/api/config", post(update_config))
        .route("/api/clients", get(list_clients))
        .route("/api/clients", post(create_client))
        .route("/api/clients/:id", get(get_client))
        .route("/api/clients/:id", post(update_client))
        .route("/api/clients/:id", delete(delete_client))
        .route("/api/providers", get(list_providers))
        .route("/api/providers", post(create_provider))
        .route("/api/providers/:id", get(get_provider))
        .route("/api/providers/:id", post(update_provider))
        .route("/api/providers/:id", delete(delete_provider))
        .route("/api/routes", get(list_routes))
        .route("/api/routes", post(create_route))
        .route("/api/routes/:id", get(get_route))
        .route("/api/routes/:id", post(update_route))
        .route("/api/routes/:id", delete(delete_route))
        .route("/api/versions", get(list_versions))
        .route("/api/versions/:version", get(get_version))
        .route("/api/versions/:version/revert", post(revert_version))
        .with_state(state)
}

async fn get_config(State(state): State<Arc<DashboardState>>) -> impl IntoResponse {
    // Retrieve the latest configuration from database
    match sqlx::query_as::<_, ConfigVersion>(
        "SELECT id, version, config_data, created_at, created_by FROM config_versions ORDER BY version DESC LIMIT 1"
    )
    .fetch_optional(&state.db_pool)
    .await
    {
        Ok(Some(version)) => ApiResponse::success(version),
        Ok(None) => ApiResponse::error("No configuration found".to_string()),
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn update_config(
    State(state): State<Arc<DashboardState>>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Validate and update configuration
    match AppConfig::load_from_value(payload) {
        Ok(config) => {
            // Serialize and store in database
            let config_data = serde_yaml::to_string(&config).map_err(|e| e.to_string())?;

            // Get current max version
            let current_version = sqlx::query!(
                "SELECT COALESCE(MAX(version), 0) as version FROM config_versions"
            )
            .fetch_one(&state.db_pool)
            .await
            .map(|r| r.version as i32)
            .unwrap_or(0);

            let new_version = current_version + 1;

            sqlx::query!(
                "INSERT INTO config_versions (version, config_data, created_by) VALUES (?, ?, ?)",
                new_version,
                config_data,
                "admin"
            )
            .execute(&state.db_pool)
            .await
            .map_err(|e| format!("Database error: {}", e))?;

            ApiResponse::success(format!("Configuration updated to version {}", new_version))
        }
        Err(e) => ApiResponse::error(format!("Invalid configuration: {}", e)),
    }
}

async fn list_clients(State(state): State<Arc<DashboardState>>) -> impl IntoResponse {
    // List all clients from database
    match sqlx::query_as::<_, ClientConfig>(
        "SELECT id, client_id, allowed_routes, requests_per_minute, concurrent_requests FROM clients"
    )
    .fetch_all(&state.db_pool)
    .await
    {
        Ok(clients) => ApiResponse::success(clients),
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn create_client(
    State(state): State<Arc<DashboardState>>,
    Json(client): Json<ClientConfig>,
) -> impl IntoResponse {
    // Create a new client
    match sqlx::query!(
        "INSERT INTO clients (client_id, allowed_routes, requests_per_minute, concurrent_requests) VALUES (?, ?, ?, ?)",
        client.id,
        client.allowed_routes.join(","),
        client.requests_per_minute,
        client.concurrent_requests
    )
    .execute(&state.db_pool)
    .await
    {
        Ok(_) => ApiResponse::success("Client created successfully"),
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn get_client(
    State(state): State<Arc<DashboardState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Get a specific client
    match sqlx::query_as::<_, ClientConfig>(
        "SELECT id, client_id, allowed_routes, requests_per_minute, concurrent_requests FROM clients WHERE client_id = ?"
    )
    .bind(&id)
    .fetch_optional(&state.db_pool)
    .await
    {
        Ok(Some(client)) => ApiResponse::success(client),
        Ok(None) => ApiResponse::error("Client not found".to_string()),
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn update_client(
    State(state): State<Arc<DashboardState>>,
    Path(id): Path<String>,
    Json(client): Json<ClientConfig>,
) -> impl IntoResponse {
    // Update a client
    match sqlx::query!(
        "UPDATE clients SET allowed_routes = ?, requests_per_minute = ?, concurrent_requests = ? WHERE client_id = ?",
        client.allowed_routes.join(","),
        client.requests_per_minute,
        client.concurrent_requests,
        id
    )
    .execute(&state.db_pool)
    .await
    {
        Ok(result) => {
            if result.rows_affected() > 0 {
                ApiResponse::success("Client updated successfully")
            } else {
                ApiResponse::error("Client not found".to_string())
            }
        }
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn delete_client(
    State(state): State<Arc<DashboardState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Delete a client
    match sqlx::query!(
        "DELETE FROM clients WHERE client_id = ?",
        id
    )
    .execute(&state.db_pool)
    .await
    {
        Ok(result) => {
            if result.rows_affected() > 0 {
                ApiResponse::success("Client deleted successfully")
            } else {
                ApiResponse::error("Client not found".to_string())
            }
        }
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn list_providers(State(state): State<Arc<DashboardState>>) -> impl IntoResponse {
    // List all providers from database
    match sqlx::query_as::<_, ProviderConfig>(
        "SELECT id, provider_id, kind, endpoint, capability_profile FROM providers"
    )
    .fetch_all(&state.db_pool)
    .await
    {
        Ok(providers) => ApiResponse::success(providers),
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn create_provider(
    State(state): State<Arc<DashboardState>>,
    Json(provider): Json<ProviderConfig>,
) -> impl IntoResponse {
    // Create a new provider
    let capability_profile = serde_json::to_string(&provider.capability_profile)
        .map_err(|e| e.to_string())?;

    match sqlx::query!(
        "INSERT INTO providers (provider_id, kind, endpoint, capability_profile, allow_insecure_http) VALUES (?, ?, ?, ?, ?)",
        provider.id,
        provider.kind.to_string(),
        provider.endpoint,
        capability_profile,
        provider.allow_insecure_http
    )
    .execute(&state.db_pool)
    .await
    {
        Ok(_) => ApiResponse::success("Provider created successfully"),
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn get_provider(
    State(state): State<Arc<DashboardState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Get a specific provider
    match sqlx::query_as::<_, ProviderConfig>(
        "SELECT id, provider_id, kind, endpoint, capability_profile FROM providers WHERE provider_id = ?"
    )
    .bind(&id)
    .fetch_optional(&state.db_pool)
    .await
    {
        Ok(Some(provider)) => ApiResponse::success(provider),
        Ok(None) => ApiResponse::error("Provider not found".to_string()),
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn update_provider(
    State(state): State<Arc<DashboardState>>,
    Path(id): Path<String>,
    Json(provider): Json<ProviderConfig>,
) -> impl IntoResponse {
    // Update a provider
    let capability_profile = serde_json::to_string(&provider.capability_profile)
        .map_err(|e| e.to_string())?;

    match sqlx::query!(
        "UPDATE providers SET kind = ?, endpoint = ?, capability_profile = ?, allow_insecure_http = ? WHERE provider_id = ?",
        provider.kind.to_string(),
        provider.endpoint,
        capability_profile,
        provider.allow_insecure_http,
        id
    )
    .execute(&state.db_pool)
    .await
    {
        Ok(result) => {
            if result.rows_affected() > 0 {
                ApiResponse::success("Provider updated successfully")
            } else {
                ApiResponse::error("Provider not found".to_string())
            }
        }
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn delete_provider(
    State(state): State<Arc<DashboardState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Delete a provider
    match sqlx::query!(
        "DELETE FROM providers WHERE provider_id = ?",
        id
    )
    .execute(&state.db_pool)
    .await
    {
        Ok(result) => {
            if result.rows_affected() > 0 {
                ApiResponse::success("Provider deleted successfully")
            } else {
                ApiResponse::error("Provider not found".to_string())
            }
        }
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn list_routes(State(state): State<Arc<DashboardState>>) -> impl IntoResponse {
    // List all routes from database
    match sqlx::query_as::<_, RouteConfig>(
        "SELECT id, route_id, models FROM routes"
    )
    .fetch_all(&state.db_pool)
    .await
    {
        Ok(routes) => ApiResponse::success(routes),
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn create_route(
    State(state): State<Arc<DashboardState>>,
    Json(route): Json<RouteConfig>,
) -> impl IntoResponse {
    // Create a new route
    match sqlx::query!(
        "INSERT INTO routes (route_id, models) VALUES (?, ?)",
        route.id,
        route.models.join(",")
    )
    .execute(&state.db_pool)
    .await
    {
        Ok(_) => ApiResponse::success("Route created successfully"),
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn get_route(
    State(state): State<Arc<DashboardState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Get a specific route
    match sqlx::query_as::<_, RouteConfig>(
        "SELECT id, route_id, models FROM routes WHERE route_id = ?"
    )
    .bind(&id)
    .fetch_optional(&state.db_pool)
    .await
    {
        Ok(Some(route)) => ApiResponse::success(route),
        Ok(None) => ApiResponse::error("Route not found".to_string()),
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn update_route(
    State(state): State<Arc<DashboardState>>,
    Path(id): Path<String>,
    Json(route): Json<RouteConfig>,
) -> impl IntoResponse {
    // Update a route
    match sqlx::query!(
        "UPDATE routes SET models = ? WHERE route_id = ?",
        route.models.join(","),
        id
    )
    .execute(&state.db_pool)
    .await
    {
        Ok(result) => {
            if result.rows_affected() > 0 {
                ApiResponse::success("Route updated successfully")
            } else {
                ApiResponse::error("Route not found".to_string())
            }
        }
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn delete_route(
    State(state): State<Arc<DashboardState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Delete a route
    match sqlx::query!(
        "DELETE FROM routes WHERE route_id = ?",
        id
    )
    .execute(&state.db_pool)
    .await
    {
        Ok(result) => {
            if result.rows_affected() > 0 {
                ApiResponse::success("Route deleted successfully")
            } else {
                ApiResponse::error("Route not found".to_string())
            }
        }
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn list_versions(State(state): State<Arc<DashboardState>>) -> impl IntoResponse {
    // List all configuration versions
    match sqlx::query_as::<_, ConfigVersion>(
        "SELECT id, version, config_data, created_at, created_by FROM config_versions ORDER BY version DESC"
    )
    .fetch_all(&state.db_pool)
    .await
    {
        Ok(versions) => ApiResponse::success(versions),
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn get_version(
    State(state): State<Arc<DashboardState>>,
    Path(version): Path<i32>,
) -> impl IntoResponse {
    // Get a specific version
    match sqlx::query_as::<_, ConfigVersion>(
        "SELECT id, version, config_data, created_at, created_by FROM config_versions WHERE version = ?"
    )
    .bind(version)
    .fetch_optional(&state.db_pool)
    .await
    {
        Ok(Some(version_data)) => ApiResponse::success(version_data),
        Ok(None) => ApiResponse::error("Version not found".to_string()),
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn revert_version(
    State(state): State<Arc<DashboardState>>,
    Path(version): Path<i32>,
) -> impl IntoResponse {
    // Revert to a specific version
    match sqlx::query!(
        "SELECT config_data FROM config_versions WHERE version = ?",
        version
    )
    .fetch_optional(&state.db_pool)
    .await
    {
        Ok(Some(record)) => {
            // Get current max version
            let current_version = sqlx::query!(
                "SELECT COALESCE(MAX(version), 0) as version FROM config_versions"
            )
            .fetch_one(&state.db_pool)
            .await
            .map(|r| r.version as i32)
            .unwrap_or(0);

            let new_version = current_version + 1;

            // Insert as new version
            sqlx::query!(
                "INSERT INTO config_versions (version, config_data, created_by) VALUES (?, ?, ?)",
                new_version,
                record.config_data,
                "admin"
            )
            .execute(&state.db_pool)
            .await
            .map_err(|e| format!("Database error: {}", e))?;

            ApiResponse::success(format!("Reverted to version {} as version {}", version, new_version))
        }
        Ok(None) => ApiResponse::error("Version not found".to_string()),
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

