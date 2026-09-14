use crate::{
    error::{ApiError, ApiResult},
    state::{AppState, SessionUser, session_token, slugify},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use starter_auth::{
    OidcDiscovery, OidcIdClaims, OidcTokenResponse, hash_password, hash_token, pkce_challenge,
    random_token, verify_password, verify_pkce,
};
use url::Url;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/providers", get(providers))
        .route(
            "/api/v1/auth/verification/request",
            post(request_verification),
        )
        .route(
            "/api/v1/auth/verification/confirm",
            post(confirm_verification),
        )
        .route("/api/v1/auth/password/forgot", post(forgot_password))
        .route("/api/v1/auth/password/reset", post(reset_password))
        .route("/api/v1/auth/oidc/{provider}/start", get(oidc_start))
        .route("/api/v1/auth/oidc/{provider}/callback", get(oidc_callback))
        .route(
            "/api/v1/auth/session-handoffs/consume",
            post(consume_handoff),
        )
        .route("/api/v1/me", get(me))
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    name: String,
    email: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct AuthResponse {
    session_token: String,
    user: PublicUser,
    workspace_id: Uuid,
    project_id: Uuid,
}

#[derive(Debug, Serialize)]
struct PublicUser {
    id: Uuid,
    email: String,
    name: String,
    email_verified: bool,
}

async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> ApiResult<(axum::http::StatusCode, Json<AuthResponse>)> {
    let name = request.name.trim();
    let email = normalize_email(&request.email)?;
    validate_name(name)?;
    validate_password(&request.password)?;
    let existing = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM users WHERE lower(email) = lower($1))",
    )
    .bind(&email)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::internal)?;
    if existing {
        return Err(ApiError::conflict(
            "email_in_use",
            "An account already exists for this email",
        ));
    }

    let user_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let membership_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let suffix = &workspace_id.to_string()[..8];
    let workspace_slug = format!("{}-{suffix}", slugify(name));
    let password_hash = hash_user_password(request.password.clone()).await?;
    let mut transaction = state.db.begin().await.map_err(ApiError::internal)?;
    sqlx::query("INSERT INTO users (user_id,email,name,password_hash) VALUES ($1,$2,$3,$4)")
        .bind(user_id)
        .bind(&email)
        .bind(name)
        .bind(password_hash)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    sqlx::query("INSERT INTO workspaces (workspace_id,name,slug,created_by) VALUES ($1,$2,$3,$4)")
        .bind(workspace_id)
        .bind(format!("{name}'s workspace"))
        .bind(workspace_slug)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    sqlx::query(
        "INSERT INTO memberships (membership_id,workspace_id,user_id,role) VALUES ($1,$2,$3,'owner')",
    )
    .bind(membership_id)
    .bind(workspace_id)
    .bind(user_id)
    .execute(&mut *transaction)
    .await
    .map_err(ApiError::internal)?;
    sqlx::query(
        "INSERT INTO projects (project_id,workspace_id,name,slug,created_by) VALUES ($1,$2,'Default','default',$3)",
    )
    .bind(project_id)
    .bind(workspace_id)
    .bind(user_id)
    .execute(&mut *transaction)
    .await
    .map_err(ApiError::internal)?;
    transaction.commit().await.map_err(ApiError::internal)?;

    send_verification(&state, user_id, &email).await?;
    let session_token = state
        .create_session(user_id, workspace_id, project_id)
        .await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(AuthResponse {
            session_token,
            user: PublicUser {
                id: user_id,
                email,
                name: name.to_string(),
                email_verified: false,
            },
            workspace_id,
            project_id,
        }),
    ))
}

async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> ApiResult<Json<AuthResponse>> {
    let email = normalize_email(&request.email)?;
    let row = sqlx::query(
        r#"
        SELECT u.user_id,u.email,u.name,u.password_hash,u.email_verified_at,
               m.workspace_id,m.role,p.project_id
        FROM users u
        JOIN memberships m ON m.user_id = u.user_id
        JOIN projects p ON p.workspace_id = m.workspace_id
        WHERE lower(u.email) = lower($1)
        ORDER BY m.created_at, p.created_at
        LIMIT 1
        "#,
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::internal)?;
    let Some(row) = row else {
        // Keep unknown-email attempts in the same expensive Argon2 class as a bad password.
        let _ = hash_user_password(request.password).await?;
        return Err(ApiError::unauthorized("Email or password is incorrect"));
    };
    let password_hash = row.get::<Option<String>, _>("password_hash");
    let password_matches = match password_hash {
        Some(hash) => verify_user_password(request.password, hash).await?,
        None => {
            let _ = hash_user_password(request.password).await?;
            false
        }
    };
    if !password_matches {
        return Err(ApiError::unauthorized("Email or password is incorrect"));
    }
    let user_id: Uuid = row.get("user_id");
    let workspace_id: Uuid = row.get("workspace_id");
    let project_id: Uuid = row.get("project_id");
    let session_token = state
        .create_session(user_id, workspace_id, project_id)
        .await?;
    Ok(Json(AuthResponse {
        session_token,
        user: PublicUser {
            id: user_id,
            email: row.get("email"),
            name: row.get("name"),
            email_verified: row
                .get::<Option<DateTime<Utc>>, _>("email_verified_at")
                .is_some(),
        },
        workspace_id,
        project_id,
    }))
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<axum::http::StatusCode> {
    let token = session_token(&headers)
        .ok_or_else(|| ApiError::unauthorized("A valid application session is required"))?;
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE token_hash = $1")
        .bind(hash_token(token))
        .execute(&state.db)
        .await
        .map_err(ApiError::internal)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn providers(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "providers": state.config.oidc_providers.public_labels().into_iter().map(|(slug, label)| {
            json!({"slug": slug, "label": label})
        }).collect::<Vec<_>>()
    }))
}

async fn me(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<SessionUser>> {
    Ok(Json(state.session_user(&headers).await?))
}

#[derive(Debug, Deserialize)]
struct EmailRequest {
    email: String,
}

#[derive(Debug, Deserialize)]
struct TokenRequest {
    token: String,
}

#[derive(Debug, Deserialize)]
struct ResetRequest {
    token: String,
    password: String,
}

async fn request_verification(
    State(state): State<AppState>,
    Json(request): Json<EmailRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let email = normalize_email(&request.email)?;
    if let Some(user_id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM users WHERE lower(email) = lower($1) AND email_verified_at IS NULL",
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::internal)?
    {
        send_verification(&state, user_id, &email).await?;
    }
    Ok(Json(json!({"accepted": true})))
}

async fn confirm_verification(
    State(state): State<AppState>,
    Json(request): Json<TokenRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut transaction = state.db.begin().await.map_err(ApiError::internal)?;
    let row = sqlx::query(
        "SELECT user_id FROM email_verification_tokens WHERE token_hash=$1 AND consumed_at IS NULL AND expires_at > now() FOR UPDATE",
    )
    .bind(hash_token(request.token.trim()))
    .fetch_optional(&mut *transaction)
    .await
    .map_err(ApiError::internal)?
    .ok_or_else(|| ApiError::bad_request("invalid_token", "Verification link is invalid or expired"))?;
    let user_id: Uuid = row.get("user_id");
    sqlx::query("UPDATE email_verification_tokens SET consumed_at=now() WHERE token_hash=$1")
        .bind(hash_token(request.token.trim()))
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    sqlx::query("UPDATE users SET email_verified_at=COALESCE(email_verified_at,now()),updated_at=now() WHERE user_id=$1")
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    transaction.commit().await.map_err(ApiError::internal)?;
    Ok(Json(json!({"verified": true})))
}

async fn forgot_password(
    State(state): State<AppState>,
    Json(request): Json<EmailRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let email = normalize_email(&request.email)?;
    if let Some(user_id) =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM users WHERE lower(email)=lower($1)")
            .bind(&email)
            .fetch_optional(&state.db)
            .await
            .map_err(ApiError::internal)?
    {
        let token = random_token(32);
        sqlx::query("INSERT INTO password_reset_tokens (token_hash,user_id,expires_at) VALUES ($1,$2,now()+interval '30 minutes')")
            .bind(hash_token(&token))
            .bind(user_id)
            .execute(&state.db)
            .await
            .map_err(ApiError::internal)?;
        let mut reset_url = state
            .config
            .app_url
            .join("/reset-password")
            .map_err(ApiError::internal)?;
        reset_url.query_pairs_mut().append_pair("token", &token);
        if let Err(error) = state
            .email
            .send(&email, "Reset your password", reset_url.as_str())
            .await
        {
            tracing::error!(error = %error, "password reset email delivery failed");
        }
    }
    Ok(Json(json!({"accepted": true})))
}

async fn reset_password(
    State(state): State<AppState>,
    Json(request): Json<ResetRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    validate_password(&request.password)?;
    let password_hash = hash_user_password(request.password).await?;
    let token_hash = hash_token(request.token.trim());
    let mut transaction = state.db.begin().await.map_err(ApiError::internal)?;
    let user_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM password_reset_tokens WHERE token_hash=$1 AND consumed_at IS NULL AND expires_at > now() FOR UPDATE",
    )
    .bind(&token_hash)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(ApiError::internal)?
    .ok_or_else(|| ApiError::bad_request("invalid_token", "Reset link is invalid or expired"))?;
    sqlx::query("UPDATE password_reset_tokens SET consumed_at=now() WHERE user_id=$1 AND consumed_at IS NULL")
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    sqlx::query("UPDATE users SET password_hash=$1,updated_at=now() WHERE user_id=$2")
        .bind(password_hash)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    sqlx::query("UPDATE sessions SET revoked_at=now() WHERE user_id=$1 AND revoked_at IS NULL")
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    sqlx::query("UPDATE oauth_refresh_tokens SET revoked_at=COALESCE(revoked_at,now()) WHERE grant_id IN (SELECT grant_id FROM oauth_grants WHERE user_id=$1)")
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    sqlx::query("UPDATE oauth_grants SET revoked_at=COALESCE(revoked_at,now()) WHERE user_id=$1")
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    transaction.commit().await.map_err(ApiError::internal)?;
    Ok(Json(json!({"reset": true})))
}

async fn send_verification(state: &AppState, user_id: Uuid, email: &str) -> ApiResult<()> {
    let token = random_token(32);
    sqlx::query("INSERT INTO email_verification_tokens (token_hash,user_id,expires_at) VALUES ($1,$2,now()+interval '24 hours')")
        .bind(hash_token(&token))
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(ApiError::internal)?;
    let mut verification_url = state
        .config
        .app_url
        .join("/verify-email")
        .map_err(ApiError::internal)?;
    verification_url
        .query_pairs_mut()
        .append_pair("token", &token);
    if let Err(error) = state
        .email
        .send(email, "Verify your email", verification_url.as_str())
        .await
    {
        tracing::error!(error = %error, "verification email delivery failed");
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct OidcStartQuery {
    return_to: Option<String>,
    browser_challenge: String,
}

async fn oidc_start(
    State(state): State<AppState>,
    Path(provider_slug): Path<String>,
    Query(query): Query<OidcStartQuery>,
) -> ApiResult<Redirect> {
    let provider = state
        .config
        .oidc_providers
        .get(&provider_slug)
        .cloned()
        .ok_or_else(|| ApiError::not_found("OIDC provider was not found"))?;
    if query.browser_challenge.len() != 43
        || !query
            .browser_challenge
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(ApiError::bad_request(
            "invalid_request",
            "Start sign-in from the application",
        ));
    }
    let discovery = discover(&state, &provider.issuer).await?;
    if discovery.issuer.trim_end_matches('/') != provider.issuer.trim_end_matches('/') {
        return Err(ApiError::bad_request(
            "invalid_provider",
            "OIDC discovery issuer does not match configuration",
        ));
    }
    let state_token = random_token(32);
    let nonce = random_token(24);
    let verifier = random_token(48);
    let return_to = safe_return_to(query.return_to.as_deref());
    sqlx::query("INSERT INTO oidc_login_requests (state_hash,provider,nonce,pkce_verifier,return_to,browser_challenge,expires_at) VALUES ($1,$2,$3,$4,$5,$6,now()+interval '10 minutes')")
        .bind(hash_token(&state_token))
        .bind(&provider_slug)
        .bind(&nonce)
        .bind(&verifier)
        .bind(&return_to)
        .bind(&query.browser_challenge)
        .execute(&state.db)
        .await
        .map_err(ApiError::internal)?;
    let callback = format!(
        "{}/api/v1/auth/oidc/{provider_slug}/callback",
        state.config.api_public_url.as_str().trim_end_matches('/')
    );
    let mut authorize = Url::parse(&discovery.authorization_endpoint).map_err(|_| {
        ApiError::bad_request("invalid_provider", "OIDC authorization endpoint is invalid")
    })?;
    authorize
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &provider.client_id)
        .append_pair("redirect_uri", &callback)
        .append_pair("scope", &provider.scopes.join(" "))
        .append_pair("state", &state_token)
        .append_pair("nonce", &nonce)
        .append_pair("code_challenge", &pkce_challenge(&verifier))
        .append_pair("code_challenge_method", "S256");
    Ok(Redirect::temporary(authorize.as_str()))
}

#[derive(Debug, Deserialize)]
struct OidcCallbackQuery {
    code: Option<String>,
    state: String,
    error: Option<String>,
}

async fn oidc_callback(
    State(state): State<AppState>,
    Path(provider_slug): Path<String>,
    Query(query): Query<OidcCallbackQuery>,
) -> ApiResult<Response> {
    if let Some(error) = query.error {
        let mut target = state
            .config
            .app_url
            .join("/login")
            .map_err(ApiError::internal)?;
        target.query_pairs_mut().append_pair("error", &error);
        return Ok(Redirect::temporary(target.as_str()).into_response());
    }
    let code = query
        .code
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::bad_request("invalid_callback", "OIDC callback has no code"))?;
    let row = sqlx::query("UPDATE oidc_login_requests SET consumed_at=now() WHERE state_hash=$1 AND provider=$2 AND consumed_at IS NULL AND expires_at > now() RETURNING nonce,pkce_verifier,return_to,browser_challenge")
        .bind(hash_token(&query.state))
        .bind(&provider_slug)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::bad_request("invalid_state", "OIDC state is invalid or expired"))?;
    let browser_challenge: String = row
        .get::<Option<String>, _>("browser_challenge")
        .ok_or_else(|| {
            ApiError::bad_request("invalid_state", "Restart sign-in from the application")
        })?;
    let nonce: String = row.get("nonce");
    let verifier: String = row.get("pkce_verifier");
    let return_to: String = row.get("return_to");
    let provider = state
        .config
        .oidc_providers
        .get(&provider_slug)
        .cloned()
        .ok_or_else(|| ApiError::not_found("OIDC provider was not found"))?;
    let discovery = discover(&state, &provider.issuer).await?;
    let callback = format!(
        "{}/api/v1/auth/oidc/{provider_slug}/callback",
        state.config.api_public_url.as_str().trim_end_matches('/')
    );
    let token = state
        .http
        .post(&discovery.token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", callback.as_str()),
            ("client_id", provider.client_id.as_str()),
            ("client_secret", provider.client_secret.as_str()),
            ("code_verifier", verifier.as_str()),
        ])
        .send()
        .await
        .map_err(ApiError::internal)?
        .error_for_status()
        .map_err(ApiError::internal)?
        .json::<OidcTokenResponse>()
        .await
        .map_err(ApiError::internal)?;
    let claims =
        validate_oidc_id_token(&state, &discovery, &provider.client_id, &token.id_token).await?;
    if claims.nonce != nonce || !claims.email_verified {
        return Err(ApiError::unauthorized(
            "OIDC identity did not provide a verified email and matching nonce",
        ));
    }
    let (user_id, workspace_id, project_id) =
        upsert_oidc_user(&state, &provider_slug, &claims).await?;
    let handoff = random_token(32);
    let expires_at = Utc::now()
        + chrono::Duration::from_std(state.config.handoff_ttl).map_err(ApiError::internal)?;
    sqlx::query("INSERT INTO session_handoffs (handoff_hash,user_id,workspace_id,project_id,expires_at,browser_challenge) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(hash_token(&handoff))
        .bind(user_id)
        .bind(workspace_id)
        .bind(project_id)
        .bind(expires_at)
        .bind(browser_challenge)
        .execute(&state.db)
        .await
        .map_err(ApiError::internal)?;
    let mut target = state
        .config
        .app_url
        .join("/auth/callback")
        .map_err(ApiError::internal)?;
    target
        .query_pairs_mut()
        .append_pair("handoff_code", &handoff)
        .append_pair("return_to", &return_to);
    Ok(Redirect::temporary(target.as_str()).into_response())
}

#[derive(Debug, Deserialize)]
struct HandoffRequest {
    handoff_code: String,
    browser_verifier: String,
}

async fn consume_handoff(
    State(state): State<AppState>,
    Json(request): Json<HandoffRequest>,
) -> ApiResult<Json<AuthResponse>> {
    let handoff_hash = hash_token(request.handoff_code.trim());
    let mut transaction = state.db.begin().await.map_err(ApiError::internal)?;
    let row = sqlx::query("SELECT h.user_id,h.workspace_id,h.project_id,h.browser_challenge,u.email,u.name,u.email_verified_at FROM session_handoffs h JOIN users u ON u.user_id=h.user_id WHERE h.handoff_hash=$1 AND h.consumed_at IS NULL AND h.expires_at > now() FOR UPDATE")
        .bind(&handoff_hash)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::bad_request("invalid_handoff", "Session handoff is invalid or expired"))?;
    let challenge = row
        .get::<Option<String>, _>("browser_challenge")
        .ok_or_else(|| ApiError::unauthorized("Restart sign-in from the application"))?;
    if !verify_pkce(&request.browser_verifier, &challenge) {
        return Err(ApiError::unauthorized(
            "Sign-in must finish in the browser that started it",
        ));
    }
    let user_id: Uuid = row.get("user_id");
    let workspace_id: Uuid = row.get("workspace_id");
    let project_id: Uuid = row.get("project_id");
    let session_token = random_token(32);
    let expires_at = Utc::now()
        + chrono::Duration::from_std(state.config.session_ttl).map_err(ApiError::internal)?;
    sqlx::query("UPDATE session_handoffs SET consumed_at=now() WHERE handoff_hash=$1")
        .bind(&handoff_hash)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    sqlx::query("INSERT INTO sessions (session_id,token_hash,user_id,active_workspace_id,active_project_id,expires_at) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(Uuid::new_v4())
        .bind(hash_token(&session_token))
        .bind(user_id)
        .bind(workspace_id)
        .bind(project_id)
        .bind(expires_at)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    transaction.commit().await.map_err(ApiError::internal)?;
    Ok(Json(AuthResponse {
        session_token,
        user: PublicUser {
            id: user_id,
            email: row.get("email"),
            name: row.get("name"),
            email_verified: row
                .get::<Option<DateTime<Utc>>, _>("email_verified_at")
                .is_some(),
        },
        workspace_id,
        project_id,
    }))
}

async fn discover(state: &AppState, issuer: &str) -> ApiResult<OidcDiscovery> {
    let issuer = Url::parse(issuer)
        .map_err(|_| ApiError::bad_request("invalid_provider", "OIDC issuer is invalid"))?;
    let url = Url::parse(&format!(
        "{}/.well-known/openid-configuration",
        issuer.as_str().trim_end_matches('/')
    ))
    .map_err(ApiError::internal)?;
    let discovery = state
        .http
        .get(url)
        .send()
        .await
        .map_err(ApiError::internal)?
        .error_for_status()
        .map_err(ApiError::internal)?
        .json()
        .await
        .map_err(ApiError::internal)?;
    validate_discovery(state, &issuer, discovery)
}

fn validate_discovery(
    state: &AppState,
    configured_issuer: &Url,
    discovery: OidcDiscovery,
) -> ApiResult<OidcDiscovery> {
    if discovery.issuer.trim_end_matches('/') != configured_issuer.as_str().trim_end_matches('/') {
        return Err(ApiError::bad_request(
            "invalid_provider",
            "OIDC discovery issuer does not match configuration",
        ));
    }
    for endpoint in [
        &discovery.authorization_endpoint,
        &discovery.token_endpoint,
        &discovery.jwks_uri,
    ] {
        let endpoint = Url::parse(endpoint).map_err(|_| {
            ApiError::bad_request("invalid_provider", "OIDC discovery endpoint is invalid")
        })?;
        if state.config.production() && endpoint.scheme() != "https" {
            return Err(ApiError::bad_request(
                "invalid_provider",
                "OIDC discovery endpoints must use HTTPS in production",
            ));
        }
    }
    Ok(discovery)
}

async fn validate_oidc_id_token(
    state: &AppState,
    discovery: &OidcDiscovery,
    client_id: &str,
    id_token: &str,
) -> ApiResult<OidcIdClaims> {
    let header =
        decode_header(id_token).map_err(|_| ApiError::unauthorized("OIDC ID token is invalid"))?;
    if !matches!(header.alg, Algorithm::RS256 | Algorithm::ES256) {
        return Err(ApiError::unauthorized(
            "OIDC signing algorithm is not allowed",
        ));
    }
    let kid = header
        .kid
        .as_deref()
        .ok_or_else(|| ApiError::unauthorized("OIDC ID token has no key identifier"))?;
    let jwks = state
        .http
        .get(&discovery.jwks_uri)
        .send()
        .await
        .map_err(ApiError::internal)?
        .error_for_status()
        .map_err(ApiError::internal)?
        .json::<JwkSet>()
        .await
        .map_err(ApiError::internal)?;
    let jwk = jwks
        .find(kid)
        .ok_or_else(|| ApiError::unauthorized("OIDC signing key is unknown"))?;
    let key = DecodingKey::from_jwk(jwk)
        .map_err(|_| ApiError::unauthorized("OIDC signing key is invalid"))?;
    let mut validation = Validation::new(header.alg);
    validation.set_issuer(&[discovery.issuer.as_str()]);
    validation.set_audience(&[client_id]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    let claims = decode::<OidcIdClaims>(id_token, &key, &validation)
        .map(|token| token.claims)
        .map_err(|_| ApiError::unauthorized("OIDC ID token validation failed"))?;
    let multiple_audiences = claims.aud.as_array().is_some_and(|values| values.len() > 1);
    if (multiple_audiences || claims.azp.is_some()) && claims.azp.as_deref() != Some(client_id) {
        return Err(ApiError::unauthorized(
            "OIDC authorized party does not match this client",
        ));
    }
    Ok(claims)
}

async fn upsert_oidc_user(
    state: &AppState,
    provider_slug: &str,
    claims: &OidcIdClaims,
) -> ApiResult<(Uuid, Uuid, Uuid)> {
    if let Some(row) = sqlx::query(
        "SELECT u.user_id,m.workspace_id,p.project_id FROM federated_identities f JOIN users u ON u.user_id=f.user_id JOIN memberships m ON m.user_id=u.user_id JOIN projects p ON p.workspace_id=m.workspace_id WHERE f.issuer=$1 AND f.subject=$2 ORDER BY m.created_at,p.created_at LIMIT 1",
    )
    .bind(&claims.iss)
    .bind(&claims.sub)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::internal)?
    {
        return Ok((row.get("user_id"), row.get("workspace_id"), row.get("project_id")));
    }
    let mut transaction = state.db.begin().await.map_err(ApiError::internal)?;
    let existing = sqlx::query(
        "SELECT user_id,email_verified_at FROM users WHERE lower(email)=lower($1) FOR UPDATE",
    )
    .bind(&claims.email)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(ApiError::internal)?;
    let (user_id, workspace_id, project_id) = if let Some(row) = existing {
        // Never attach an external identity to an unverified password account:
        // an attacker may have registered that email before its real owner.
        if row
            .get::<Option<DateTime<Utc>>, _>("email_verified_at")
            .is_none()
        {
            return Err(ApiError::forbidden(
                "Verify the existing account before linking this identity",
            ));
        }
        let user_id: Uuid = row.get("user_id");
        let membership = sqlx::query("SELECT m.workspace_id,p.project_id FROM memberships m JOIN projects p ON p.workspace_id=m.workspace_id WHERE m.user_id=$1 ORDER BY m.created_at,p.created_at LIMIT 1")
            .bind(user_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(ApiError::internal)?;
        (
            user_id,
            membership.get("workspace_id"),
            membership.get("project_id"),
        )
    } else {
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let name = claims
            .name
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| claims.email.split('@').next().unwrap_or("User"));
        let suffix = &workspace_id.to_string()[..8];
        sqlx::query(
            "INSERT INTO users (user_id,email,name,email_verified_at) VALUES ($1,$2,$3,now())",
        )
        .bind(user_id)
        .bind(claims.email.to_ascii_lowercase())
        .bind(name)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
        sqlx::query(
            "INSERT INTO workspaces (workspace_id,name,slug,created_by) VALUES ($1,$2,$3,$4)",
        )
        .bind(workspace_id)
        .bind(format!("{name}'s workspace"))
        .bind(format!("{}-{suffix}", slugify(name)))
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
        sqlx::query("INSERT INTO memberships (membership_id,workspace_id,user_id,role) VALUES ($1,$2,$3,'owner')")
            .bind(Uuid::new_v4())
            .bind(workspace_id)
            .bind(user_id)
            .execute(&mut *transaction)
            .await
            .map_err(ApiError::internal)?;
        sqlx::query("INSERT INTO projects (project_id,workspace_id,name,slug,created_by) VALUES ($1,$2,'Default','default',$3)")
            .bind(project_id)
            .bind(workspace_id)
            .bind(user_id)
            .execute(&mut *transaction)
            .await
            .map_err(ApiError::internal)?;
        (user_id, workspace_id, project_id)
    };
    sqlx::query("INSERT INTO federated_identities (federated_identity_id,user_id,provider,issuer,subject,email) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(provider_slug)
        .bind(&claims.iss)
        .bind(&claims.sub)
        .bind(&claims.email)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    transaction.commit().await.map_err(ApiError::internal)?;
    Ok((user_id, workspace_id, project_id))
}

fn normalize_email(value: &str) -> ApiResult<String> {
    let email = value.trim().to_ascii_lowercase();
    if email.len() > 254 || !email.contains('@') || email.starts_with('@') || email.ends_with('@') {
        return Err(ApiError::bad_request(
            "invalid_email",
            "Enter a valid email address",
        ));
    }
    Ok(email)
}

fn validate_name(value: &str) -> ApiResult<()> {
    if !(2..=100).contains(&value.chars().count()) {
        return Err(ApiError::bad_request(
            "invalid_name",
            "Name must be between 2 and 100 characters",
        ));
    }
    Ok(())
}

fn validate_password(value: &str) -> ApiResult<()> {
    if value.len() < 8 || value.len() > 1024 {
        return Err(ApiError::bad_request(
            "invalid_password",
            "Password must be at least 8 characters",
        ));
    }
    Ok(())
}

fn safe_return_to(value: Option<&str>) -> String {
    value
        .filter(|value| {
            value.starts_with('/')
                && !value.starts_with("//")
                && !value
                    .chars()
                    .any(|c| c == '\\' || c.is_ascii_control() || c == ' ')
        })
        .unwrap_or("/dashboard")
        .to_string()
}

async fn hash_user_password(password: String) -> ApiResult<String> {
    tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)
}

async fn verify_user_password(password: String, hash: String) -> ApiResult<bool> {
    tokio::task::spawn_blocking(move || verify_password(&password, &hash))
        .await
        .map_err(ApiError::internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_paths_reject_browser_origin_escapes() {
        for target in [
            "//evil.example",
            "/\\evil.example",
            "/\n/evil.example",
            "/\t/evil.example",
            "https://evil.example",
        ] {
            assert_eq!(safe_return_to(Some(target)), "/dashboard");
        }
        assert_eq!(
            safe_return_to(Some("/oauth/consent?request_id=1")),
            "/oauth/consent?request_id=1"
        );
    }
}
