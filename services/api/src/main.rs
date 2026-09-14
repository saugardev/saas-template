mod config;
mod email;
mod error;
mod routes;
mod state;

use anyhow::Context;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderValue, Method, Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use config::Config;
use email::EmailSender;
use reqwest::redirect::Policy;
use serde_json::json;
use starter_auth::JwtIssuer;
use state::AppState;
use std::{fs, sync::Arc, time::Duration};
use tower_http::{
    catch_panic::CatchPanicLayer, cors::CorsLayer,
    sensitive_headers::SetSensitiveRequestHeadersLayer, timeout::TimeoutLayer, trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();
    let config = Arc::new(Config::from_env()?);
    let email = EmailSender::from_config(&config)?;
    let private_pem = fs::read(&config.private_key_path).with_context(|| {
        format!(
            "read private key at {}; run `bun run keys:dev` for local development",
            config.private_key_path.display()
        )
    })?;
    let public_pem = fs::read(&config.public_key_path)
        .with_context(|| format!("read public key at {}", config.public_key_path.display()))?;
    let jwt = JwtIssuer::from_pem(
        config.auth_issuer.as_str().trim_end_matches('/'),
        config.key_id.clone(),
        &private_pem,
        &public_pem,
    )?;
    let db = starter_db::connect(&config.database_url).await?;
    starter_db::migrate(&db).await?;
    let http = reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(8))
        .user_agent("agent-saas-starter/0.1")
        .build()?;
    let state = AppState {
        config: config.clone(),
        db,
        jwt,
        http,
        email,
    };

    let app_origin = HeaderValue::from_str(config.app_url.origin().ascii_serialization().as_str())?;
    let app = Router::new()
        .route("/healthz", get(|| async { Json(json!({"status": "ok"})) }))
        .route("/readyz", get(ready))
        .merge(routes::auth::router())
        .merge(routes::workspaces::router())
        .merge(routes::oauth::router())
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(128 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), host_guard))
        .layer(middleware::from_fn(security_headers))
        .layer(
            CorsLayer::new()
                .allow_origin(app_origin)
                .allow_methods([Method::GET, Method::POST, Method::DELETE])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]),
        )
        .layer(SetSensitiveRequestHeadersLayer::new(std::iter::once(
            header::AUTHORIZATION,
        )))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(20),
        ))
        .layer(CatchPanicLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("bind API on {}", config.bind))?;
    tracing::info!(address = %config.bind, issuer = %config.auth_issuer, "API listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    if starter_db::ready(&state.db).await {
        (axum::http::StatusCode::OK, Json(json!({"status": "ready"})))
    } else {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "unavailable"})),
        )
    }
}

async fn not_found() -> impl IntoResponse {
    (
        axum::http::StatusCode::NOT_FOUND,
        Json(json!({
            "error": {
                "code": "not_found",
                "message": "Route was not found",
                "request_id": uuid::Uuid::new_v4()
            }
        })),
    )
}

async fn host_guard(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if state.config.production() {
        let actual = request
            .headers()
            .get(header::HOST)
            .and_then(|value| value.to_str().ok());
        if !allowed_host(
            actual,
            &state.config.auth_issuer,
            &state.config.api_internal_url,
        ) {
            return (
                axum::http::StatusCode::MISDIRECTED_REQUEST,
                Json(json!({"error": {"code": "invalid_host", "message": "Host is not configured for this authorization server"}})),
            )
                .into_response();
        }
    }
    next.run(request).await
}

fn allowed_host(actual: Option<&str>, public: &url::Url, internal: &url::Url) -> bool {
    let Some(actual) = actual.and_then(|value| value.parse::<axum::http::uri::Authority>().ok())
    else {
        return false;
    };
    if actual.as_str().contains('@') || url::Url::parse(&format!("http://{actual}")).is_err() {
        return false;
    }
    [public, internal].iter().any(|url| {
        url.host_str()
            .is_some_and(|host| actual.host().eq_ignore_ascii_case(host))
    })
}

async fn security_headers(request: Request<axum::body::Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "starter_api=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_configured_public_and_internal_hosts_are_accepted() {
        let public = url::Url::parse("https://api.example.test").unwrap();
        let internal = url::Url::parse("http://127.0.0.1:4000").unwrap();
        for value in ["api.example.test", "API.EXAMPLE.TEST:443", "127.0.0.1:4000"] {
            assert!(super::allowed_host(Some(value), &public, &internal));
        }
        for value in [
            None,
            Some("evil.example"),
            Some("api.example.test.evil"),
            Some("api.example.test:invalid"),
        ] {
            assert!(!super::allowed_host(value, &public, &internal));
        }
    }
}
