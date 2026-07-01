use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{delete, get, post},
    service::get_service,
    Json, Router,
};
use argon2::{Argon2, Algorithm, Version};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions, SqliteRow};
use sqlx::{Row};
use tower_http::services::ServeDir;
use tower_cookies::{Cookie, CookieManagerLayer, Cookies};

use crate::{
    config::{AppConfig, ClientConfig, ProviderConfig, ProviderKind, RouteConfig},
    error::{ErrorKind, ProxyError, Result},
};

pub struct DashboardState {
    pub db_pool: SqlitePool,
    pub admin_secret: String,
}

pub fn build_dashboard_routes(state: Arc<DashboardState>) -> Router {
    Router::new()
        .route("/dashboard", get(dashboard_index))
        .route("/dashboard/login", get(login_page).post(login_handler))
        .route("/dashboard/logout", get(logout_handler))
        .route("/api/config", get(get_config).post(update_config))
        .route("/api/clients", get(list_clients).post(create_client))
        .route("/api/clients/:id", get(get_client).put(update_client).delete(delete_client))
        .route("/api/providers", get(list_providers).post(create_provider))
        .route("/api/providers/:id", get(get_provider).put(update_provider).delete(delete_provider))
        .route("/api/routes", get(list_routes).post(create_route))
        .route("/api/routes/:id", get(get_route).put(update_route).delete(delete_route))
        .route("/api/versions", get(list_versions))
        .route("/api/versions/:version", get(get_version))
        .route("/api/versions/:version/revert", post(revert_version))
        .nest_service(
            "/static",
            get_service(ServeDir::new("static")).handle_error(|error| async move {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to serve static file: {}", error),
                )
            }),
        )
        .route_layer(CookieManagerLayer::new())
        .with_state(state)
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

#[derive(Debug, Clone, Serialize)]
struct DbRoute {
    pub id: String,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DbClient {
    pub id: String,
    pub allowed_routes: Vec<String>,
    pub requests_per_minute: u32,
    pub concurrent_requests: usize,
}

#[derive(Debug, Clone, Serialize)]
struct DbProvider {
    pub id: String,
    pub kind: String,
    pub endpoint: String,
    pub capability_profile: String,
    pub allow_insecure_http: bool,
}

fn split_comma_separated(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(String::from)
        .collect()
}

fn row_to_config_version(row: &SqliteRow) -> ConfigVersion {
    ConfigVersion {
        id: row.get("id"),
        version: row.get("version"),
        config_data: row.get("config_data"),
        created_at: row.get("created_at"),
        created_by: row.get("created_by"),
        is_current: row.get("is_current"),
    }
}

fn row_to_route(row: &SqliteRow) -> DbRoute {
    DbRoute {
        id: row.get("route_id"),
        models: split_comma_separated(&row.get::<String, _>("models")),
    }
}

fn row_to_client(row: &SqliteRow) -> DbClient {
    DbClient {
        id: row.get("client_id"),
        allowed_routes: split_comma_separated(&row.get::<String, _>("allowed_routes")),
        requests_per_minute: row.get("requests_per_minute"),
        concurrent_requests: row.get::<u32, _>("concurrent_requests") as usize,
    }
}

fn row_to_provider(row: &SqliteRow) -> DbProvider {
    DbProvider {
        id: row.get("provider_id"),
        kind: row.get("kind"),
        endpoint: row.get("endpoint"),
        capability_profile: row.get("capability_profile"),
        allow_insecure_http: row.get("allow_insecure_http"),
    }
}

fn row_to_auth_user(row: &SqliteRow) -> AuthUser {
    AuthUser {
        id: row.get("id"),
        username: row.get("username"),
        password_hash: row.get("password_hash"),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigVersion {
    pub id: i64,
    pub version: i32,
    pub config_data: String,
    pub created_at: String,
    pub created_by: String,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
}

pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| {
            ProxyError::new(
                ErrorKind::Internal,
                format!("Failed to hash password: {}", e),
            )
        })?
        .to_string();

    Ok(password_hash)
}

pub async fn create_initial_admin_user(pool: &SqlitePool, admin_secret: &str) -> Result<()> {
    // Check if admin user exists
    let user_exists = sqlx::query(
        "SELECT 1 FROM admin_users WHERE username = 'admin'"
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        ProxyError::new(
            ErrorKind::Internal,
            format!("Failed to check for admin user: {}", e),
        )
    })?
    .is_some();

    if !user_exists {
        // Hash the admin secret
        let password_hash = hash_password(admin_secret)?;

        // Create the admin user
        sqlx::query("INSERT INTO admin_users (username, password_hash) VALUES (?, ?)")
            .bind("admin")
            .bind(password_hash)
        .execute(pool)
        .await
        .map_err(|e| {
            ProxyError::new(
                ErrorKind::Internal,
                format!("Failed to create admin user: {}", e),
            )
        })?;
    }

    Ok(())
}

pub async fn create_sample_config(pool: &SqlitePool) -> Result<()> {
    // Check if any config exists
    let config_exists = sqlx::query(
        "SELECT 1 FROM config_versions LIMIT 1"
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        ProxyError::new(
            ErrorKind::Internal,
            format!("Failed to check for existing config: {}", e),
        )
    })?
    .is_some();

    if !config_exists {
        // Create a sample configuration
        let sample_config = AppConfig {
            server: Default::default(),
            limits: Default::default(),
            clients: vec![
                ClientConfig {
                    id: "engineering".to_string(),
                    key: crate::config::SecretRef::Env {
                        env: "PROXY_CLIENT_KEY".to_string(),
                    },
                    allowed_routes: vec!["sonnet".to_string(), "haiku".to_string()],
                    requests_per_minute: 600,
                    concurrent_requests: 100,
                },
            ],
            providers: vec![
                ProviderConfig {
                    id: "openai".to_string(),
                    kind: crate::config::ProviderKind::OpenaiChat,
                    endpoint: "https://api.openai.com/v1".to_string(),
                    credential: crate::config::CredentialConfig::Bearer {
                        secret: crate::config::SecretRef::Env {
                            env: "OPENAI_API_KEY".to_string(),
                        },
                    },
                    headers: Default::default(),
                    capability_profile: Default::default(),
                    allow_insecure_http: false,
                },
            ],
            routes: vec![
                RouteConfig {
                    id: "sonnet".to_string(),
                    models: vec!["claude-*-sonnet-*".to_string(), "claude-sonnet-*".to_string()],
                    targets: vec![
                        crate::config::TargetConfig {
                            provider: "openai".to_string(),
                            model: "gpt-4.1".to_string(),
                            priority: 1,
                            weight: 100,
                        },
                    ],
                },
                RouteConfig {
                    id: "haiku".to_string(),
                    models: vec!["claude-*-haiku-*".to_string(), "claude-haiku-*".to_string()],
                    targets: vec![
                        crate::config::TargetConfig {
                            provider: "openai".to_string(),
                            model: "gpt-4.1-mini".to_string(),
                            priority: 1,
                            weight: 100,
                        },
                    ],
                },
            ],
        };

        // Serialize the sample config
        let config_data = serde_yaml::to_string(&sample_config).map_err(|e| {
            ProxyError::new(
                ErrorKind::Internal,
                format!("Failed to serialize sample config: {}", e),
            )
        })?;

        // Insert the sample config
        sqlx::query("INSERT INTO config_versions (version, config_data, created_by) VALUES (1, ?, 'system')")
            .bind(config_data)
        .execute(pool)
        .await
        .map_err(|e| {
            ProxyError::new(
                ErrorKind::Internal,
                format!("Failed to create sample config: {}", e),
            )
        })?;
    }

    Ok(())
}

pub async fn create_sample_client(pool: &SqlitePool) -> Result<()> {
    // Check if any clients exist
    let client_exists = sqlx::query(
        "SELECT 1 FROM clients LIMIT 1"
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        ProxyError::new(
            ErrorKind::Internal,
            format!("Failed to check for existing clients: {}", e),
        )
    })?
    .is_some();

    if !client_exists {
        // Create a sample client
        sqlx::query("INSERT INTO clients (client_id, key_hash, key_salt, allowed_routes, requests_per_minute, concurrent_requests) VALUES (?, ?, ?, ?, ?, ?)")
            .bind("engineering")
            .bind("sample_hash")
            .bind("sample_salt")
            .bind("sonnet,haiku")
            .bind(600_i32)
            .bind(100_i32)
        .execute(pool)
        .await
        .map_err(|e| {
            ProxyError::new(
                ErrorKind::Internal,
                format!("Failed to create sample client: {}", e),
            )
        })?;
    }

    Ok(())
}

pub async fn create_sample_provider(pool: &SqlitePool) -> Result<()> {
    // Check if any providers exist
    let provider_exists = sqlx::query(
        "SELECT 1 FROM providers LIMIT 1"
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        ProxyError::new(
            ErrorKind::Internal,
            format!("Failed to check for existing providers: {}", e),
        )
    })?
    .is_some();

    if !provider_exists {
        // Create a sample provider
        let capability_profile = serde_json::to_string(&crate::config::CapabilityProfile::default()).map_err(|e| {
            ProxyError::new(
                ErrorKind::Internal,
                format!("Failed to serialize capability profile: {}", e),
            )
        })?;

        sqlx::query("INSERT INTO providers (provider_id, kind, endpoint, credential_type, credential_secret_ref, headers, capability_profile, allow_insecure_http) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind("openai")
            .bind("openai_chat")
            .bind("https://api.openai.com/v1")
            .bind("bearer")
            .bind("OPENAI_API_KEY")
            .bind("{}")
            .bind(capability_profile)
            .bind(0_i32)
        .execute(pool)
        .await
        .map_err(|e| {
            ProxyError::new(
                ErrorKind::Internal,
                format!("Failed to create sample provider: {}", e),
            )
        })?;
    }

    Ok(())
}

pub async fn create_sample_route(pool: &SqlitePool) -> Result<()> {
    // Check if any routes exist
    let route_exists = sqlx::query(
        "SELECT 1 FROM routes LIMIT 1"
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        ProxyError::new(
            ErrorKind::Internal,
            format!("Failed to check for existing routes: {}", e),
        )
    })?
    .is_some();

    if !route_exists {
        // Create a sample route
        sqlx::query("INSERT INTO routes (route_id, models) VALUES (?, ?)")
            .bind("sonnet")
            .bind("claude-*-sonnet-*,claude-sonnet-*")
        .execute(pool)
        .await
        .map_err(|e| {
            ProxyError::new(
                ErrorKind::Internal,
                format!("Failed to create sample route: {}", e),
            )
        })?;

        // Get the route ID
let route_id: i64 = sqlx::query("SELECT id FROM routes WHERE route_id = 'sonnet'")
            .fetch_one(pool)
            .await
            .map_err(|e| {
                ProxyError::new(
                    ErrorKind::Internal,
                    format!("Failed to get route ID: {}", e),
                )
            })?
            .get("id");

        // Create a target for the route
        sqlx::query("INSERT INTO route_targets (route_id, provider_id, model, priority, weight) VALUES (?, ?, ?, ?, ?)")
            .bind(route_id)
            .bind("openai")
            .bind("gpt-4.1")
            .bind(1_i32)
            .bind(100_i32)
        .execute(pool)
        .await
        .map_err(|e| {
            ProxyError::new(
                ErrorKind::Internal,
                format!("Failed to create route target: {}", e),
            )
        })?;
    }

    Ok(())
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

pub async fn health_check(State(state): State<Arc<DashboardState>>) -> impl IntoResponse {
    // Check database connection
    match sqlx::query("SELECT 1").fetch_optional(&state.db_pool).await {
        Ok(_) => {
            // Database is healthy
            ApiResponse::success("Proxy is healthy")
        }
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

async fn login_page() -> impl IntoResponse {
    Html(std::include_str!("../templates/login.html"))
}

async fn login_handler(
    State(state): State<Arc<DashboardState>>,
    Json(payload): Json<LoginRequest>,
    cookies: Cookies,
) -> impl IntoResponse {
    // Validate input
    if payload.username.is_empty() || payload.password.is_empty() {
        return ApiResponse::error("Username and password are required".to_string()).into_response();
    }

    // Check if the username and password match the admin credentials
    if payload.username == "admin" && payload.password == state.admin_secret {
        // Set a session cookie
        let cookie = Cookie::build("auth_token", "authenticated")
            .path("/")
            .http_only(true)
            .finish();
        cookies.add(cookie);

        // Redirect to dashboard
        Redirect::to("/dashboard")
    } else {
        // Check if the user exists in the database
        match sqlx::query(
            "SELECT id, username, password_hash FROM admin_users WHERE username = ?"
        )
        .bind(&payload.username)
        .fetch_optional(&state.db_pool)
        .await
        .map(|opt| opt.map(|row| row_to_auth_user(&row)))
        {
            Ok(Some(user)) => {
                // Verify password
                if PasswordHash::new(&user.password_hash)
                    .and_then(|parsed| Argon2::default().verify_password(payload.password.as_bytes(), &parsed))
                    .is_ok()
                {
                    // Set a session cookie
                    let cookie = Cookie::build("auth_token", "authenticated")
                        .path("/")
                        .http_only(true)
                        .finish();
                    cookies.add(cookie);

                    // Redirect to dashboard
                    Redirect::to("/dashboard")
                } else {
                    ApiResponse::error("Invalid credentials".to_string()).into_response()
                }
            }
            Ok(None) => ApiResponse::error("Invalid credentials".to_string()).into_response(),
            Err(e) => ApiResponse::error(format!("Database error: {}", e)).into_response(),
        }
    }
}

async fn logout_handler(cookies: Cookies) -> impl IntoResponse {
    // Clear the auth cookie
    cookies.remove(Cookie::build("auth_token", "").path("/").finish());

    // Redirect to login page
    Redirect::to("/dashboard/login")
}

async fn auth_middleware(
    cookies: Cookies,
    request: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<Response> {
    // Skip auth for login page and static files
    if request.uri().path().starts_with("/dashboard/login") || request.uri().path().starts_with("/static/") {
        return Ok(next.run(request).await);
    }

    // Check for auth cookie
    if let Some(cookie) = cookies.get("auth_token") {
        if cookie.value() == "authenticated" {
            return Ok(next.run(request).await);
        }
    }

    // Redirect to login if not authenticated
    Ok(Redirect::to("/dashboard/login").into_response())
}

async fn dashboard_index(cookies: Cookies) -> impl IntoResponse {
    // Check if user is authenticated
    if let Some(cookie) = cookies.get("auth_token") {
        if cookie.value() == "authenticated" {
            return Html(std::include_str!("../templates/index.html"));
        }
    }

    // Redirect to login if not authenticated
    Redirect::to("/dashboard/login")
}

async fn get_config(State(state): State<Arc<DashboardState>>) -> impl IntoResponse {
    // Retrieve the latest configuration from database
    match sqlx::query(
        "SELECT id, version, config_data, created_at, created_by, is_current FROM config_versions ORDER BY version DESC LIMIT 1"
    )
    .fetch_optional(&state.db_pool)
    .await
    .map(|opt| opt.map(|row| row_to_config_version(&row)))
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
            let config_data = match serde_yaml::to_string(&config) {
                Ok(data) => data,
                Err(e) => return ApiResponse::error(format!("Invalid configuration: {}", e)).into_response(),
            };

            // Get current max version
            let current_version = sqlx::query(
                "SELECT COALESCE(MAX(version), 0) as version FROM config_versions"
            )
            .fetch_one(&state.db_pool)
            .await
            .map(|r| r.get::<i32, _>("version") as i32)
            .unwrap_or(0);

            let new_version = current_version + 1;

            if let Err(e) = sqlx::query("INSERT INTO config_versions (version, config_data, created_by) VALUES (?, ?, ?)")
                .bind(new_version)
                .bind(config_data)
                .bind("admin")
                .execute(&state.db_pool)
                .await
            {
                return ApiResponse::error(format!("Database error: {}", e)).into_response();
            }

            ApiResponse::success(format!("Configuration updated to version {}", new_version))
        }
        Err(e) => ApiResponse::error(format!("Invalid configuration: {}", e)),
    }
}

async fn list_clients(State(state): State<Arc<DashboardState>>) -> impl IntoResponse {
    // List all clients from database
    match sqlx::query(
        "SELECT client_id, allowed_routes, requests_per_minute, concurrent_requests FROM clients"
    )
    .fetch_all(&state.db_pool)
    .await
    .map(|rows| rows.into_iter().map(|row| row_to_client(&row)).collect::<Vec<_>>())
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
    match sqlx::query("INSERT INTO clients (client_id, allowed_routes, requests_per_minute, concurrent_requests) VALUES (?, ?, ?, ?)")
        .bind(client.id)
        .bind(client.allowed_routes.join(","))
        .bind(client.requests_per_minute)
        .bind(client.concurrent_requests)
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
    match sqlx::query(
        "SELECT client_id, allowed_routes, requests_per_minute, concurrent_requests FROM clients WHERE client_id = ?"
    )
    .bind(&id)
    .fetch_optional(&state.db_pool)
    .await
    .map(|opt| opt.map(|row| row_to_client(&row)))
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
    match sqlx::query("UPDATE clients SET allowed_routes = ?, requests_per_minute = ?, concurrent_requests = ? WHERE client_id = ?")
        .bind(client.allowed_routes.join(","))
        .bind(client.requests_per_minute)
        .bind(client.concurrent_requests)
        .bind(id)
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
    match sqlx::query("DELETE FROM clients WHERE client_id = ?")
        .bind(id)
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
    match sqlx::query(
        "SELECT provider_id, kind, endpoint, capability_profile, allow_insecure_http FROM providers"
    )
    .fetch_all(&state.db_pool)
    .await
    .map(|rows| rows.into_iter().map(|row| row_to_provider(&row)).collect::<Vec<_>>())
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
    let capability_profile = match serde_json::to_string(&provider.capability_profile) {
        Ok(profile) => profile,
        Err(e) => return ApiResponse::error(format!("Invalid capability profile: {}", e)).into_response(),
    };

    match sqlx::query("INSERT INTO providers (provider_id, kind, endpoint, capability_profile, allow_insecure_http) VALUES (?, ?, ?, ?, ?)")
        .bind(provider.id)
        .bind(provider.kind.to_string())
        .bind(provider.endpoint)
        .bind(capability_profile)
        .bind(provider.allow_insecure_http)
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
    match sqlx::query(
        "SELECT provider_id, kind, endpoint, capability_profile, allow_insecure_http FROM providers WHERE provider_id = ?"
    )
    .bind(&id)
    .fetch_optional(&state.db_pool)
    .await
    .map(|opt| opt.map(|row| row_to_provider(&row)))
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
    let capability_profile = match serde_json::to_string(&provider.capability_profile) {
        Ok(profile) => profile,
        Err(e) => return ApiResponse::error(format!("Invalid capability profile: {}", e)).into_response(),
    };

    match sqlx::query("UPDATE providers SET kind = ?, endpoint = ?, capability_profile = ?, allow_insecure_http = ? WHERE provider_id = ?")
        .bind(provider.kind.to_string())
        .bind(provider.endpoint)
        .bind(capability_profile)
        .bind(provider.allow_insecure_http)
        .bind(id)
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
    match sqlx::query("DELETE FROM providers WHERE provider_id = ?")
        .bind(id)
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
    match sqlx::query(
        "SELECT route_id, models FROM routes"
    )
    .fetch_all(&state.db_pool)
    .await
    .map(|rows| rows.into_iter().map(|row| row_to_route(&row)).collect::<Vec<_>>())
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
    match sqlx::query("INSERT INTO routes (route_id, models) VALUES (?, ?)")
        .bind(route.id)
        .bind(route.models.join(","))
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
    match sqlx::query(
        "SELECT route_id, models FROM routes WHERE route_id = ?"
    )
    .bind(&id)
    .fetch_optional(&state.db_pool)
    .await
    .map(|opt| opt.map(|row| row_to_route(&row)))
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
    match sqlx::query("UPDATE routes SET models = ? WHERE route_id = ?")
        .bind(route.models.join(","))
        .bind(id)
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
    match sqlx::query("DELETE FROM routes WHERE route_id = ?")
        .bind(id)
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
    match sqlx::query(
        "SELECT id, version, config_data, created_at, created_by, is_current FROM config_versions ORDER BY version DESC"
    )
    .fetch_all(&state.db_pool)
    .await
    .map(|rows| rows.into_iter().map(|row| row_to_config_version(&row)).collect::<Vec<_>>())
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
    match sqlx::query(
        "SELECT id, version, config_data, created_at, created_by, is_current FROM config_versions WHERE version = ?"
    )
    .bind(version)
    .fetch_optional(&state.db_pool)
    .await
    .map(|opt| opt.map(|row| row_to_config_version(&row)))
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
    match sqlx::query("SELECT config_data FROM config_versions WHERE version = ?")
        .bind(version)
        .fetch_optional(&state.db_pool)
        .await
    {
        Ok(Some(record)) => {
            let config_data: String = record.get("config_data");
            // Get current max version
            let current_version = sqlx::query(
                "SELECT COALESCE(MAX(version), 0) as version FROM config_versions"
            )
            .fetch_one(&state.db_pool)
            .await
            .map(|r| r.get::<i32, _>("version") as i32)
            .unwrap_or(0);

            let new_version = current_version + 1;

            if let Err(e) = sqlx::query("INSERT INTO config_versions (version, config_data, created_by) VALUES (?, ?, ?)")
                .bind(new_version)
                .bind(config_data)
                .bind("admin")
                .execute(&state.db_pool)
                .await
            {
                return ApiResponse::error(format!("Database error: {}", e)).into_response();
            }

            ApiResponse::success(format!("Reverted to version {} as version {}", version, new_version))
        }
        Ok(None) => ApiResponse::error("Version not found".to_string()),
        Err(e) => ApiResponse::error(format!("Database error: {}", e)),
    }
}

