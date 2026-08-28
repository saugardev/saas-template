use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};
use axum::{
    Form, Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use starter_auth::{
    AuthContext, CIMD_MAX_BYTES, CimdClientMetadata, IdTokenClaims, OAuthClientRegistration,
    OAuthErrorRedirect, OAuthTokenRequest, OAuthTokenResponse, SUPPORTED_SCOPES,
    authorization_redirect, choose_token_auth_method, hash_token, normalize_registered_scopes,
    normalize_scopes, oauth_error_redirect, random_token, redirect_uri_matches,
    validate_cimd_redirect_uri, validate_client_metadata_url, validate_redirect_uri, verify_pkce,
};
use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};
use url::Url;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/.well-known/oauth-authorization-server",
            get(authorization_server_metadata),
        )
        .route("/.well-known/openid-configuration", get(openid_metadata))
        .route("/.well-known/jwks.json", get(jwks))
        .route("/oauth/authorize", get(authorize))
        .route("/oauth/token", post(token))
        .route("/oauth/register", post(register))
        .route("/oauth/revoke", post(revoke))
        .route("/oauth/userinfo", get(userinfo).post(userinfo))
        .route(
            "/api/v1/oauth/authorization-requests/{request_id}",
            get(consent_request).post(decide_consent),
        )
}

async fn authorization_server_metadata(State(state): State<AppState>) -> Json<Value> {
    let issuer = state.config.auth_issuer.as_str().trim_end_matches('/');
    Json(authorization_server_metadata_document(
        issuer,
        state.config.docs_url.as_str(),
        &state.config.app_name,
        state.config.dcr_enabled,
        !state.config.cimd_allowed_origins.is_empty(),
    ))
}

fn authorization_server_metadata_document(
    issuer: &str,
    docs_url: &str,
    app_name: &str,
    dcr_enabled: bool,
    cimd_enabled: bool,
) -> Value {
    let mut metadata = json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/oauth/authorize"),
        "token_endpoint": format!("{issuer}/oauth/token"),
        "revocation_endpoint": format!("{issuer}/oauth/revoke"),
        "userinfo_endpoint": format!("{issuer}/oauth/userinfo"),
        "jwks_uri": format!("{issuer}/.well-known/jwks.json"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "client_id_metadata_document_supported": cimd_enabled,
        "scopes_supported": SUPPORTED_SCOPES,
        "resource_parameter_supported": true,
        "authorization_response_iss_parameter_supported": true,
        "service_documentation": docs_url,
        "service_name": app_name,
    });
    if dcr_enabled {
        metadata["registration_endpoint"] = json!(format!("{issuer}/oauth/register"));
    }
    metadata
}

async fn openid_metadata(State(state): State<AppState>) -> Json<Value> {
    let mut metadata = authorization_server_metadata(State(state.clone())).await.0;
    metadata["subject_types_supported"] = json!(["public"]);
    metadata["id_token_signing_alg_values_supported"] = json!(["RS256"]);
    metadata["claims_supported"] = json!([
        "sub",
        "iss",
        "aud",
        "exp",
        "iat",
        "auth_time",
        "nonce",
        "email",
        "email_verified",
        "name",
        "workspace_id",
        "project_id",
        "role"
    ]);
    Json(metadata)
}

async fn jwks(State(state): State<AppState>) -> Json<Value> {
    Json(json!({"keys": [state.jwt.jwk()]}))
}

#[derive(Debug, Serialize)]
struct RegistrationResponse {
    client_id: String,
    client_name: String,
    client_uri: Option<String>,
    redirect_uris: Vec<String>,
    grant_types: Vec<&'static str>,
    response_types: Vec<&'static str>,
    token_endpoint_auth_method: &'static str,
    scope: String,
    client_id_issued_at: i64,
}

async fn register(
    State(state): State<AppState>,
    Json(request): Json<OAuthClientRegistration>,
) -> ApiResult<(StatusCode, Json<RegistrationResponse>)> {
    if !state.config.dcr_enabled {
        return Err(ApiError::not_found(
            "Dynamic client registration is disabled",
        ));
    }
    let client_name = request.client_name.trim();
    if client_name.is_empty() || client_name.len() > 120 {
        return Err(ApiError::bad_request(
            "invalid_client_metadata",
            "client_name must be between 1 and 120 characters",
        ));
    }
    if request.redirect_uris.is_empty() || request.redirect_uris.len() > 10 {
        return Err(ApiError::bad_request(
            "invalid_redirect_uri",
            "Provide between 1 and 10 redirect URIs",
        ));
    }
    for redirect in &request.redirect_uris {
        validate_redirect_uri(redirect)
            .map_err(|error| ApiError::bad_request("invalid_redirect_uri", error.to_string()))?;
    }
    if request.token_endpoint_auth_method != "none" {
        return Err(ApiError::bad_request(
            "invalid_client_metadata",
            "This starter registers public clients only",
        ));
    }
    if !request.grant_types.is_empty()
        && request
            .grant_types
            .iter()
            .any(|value| value != "authorization_code" && value != "refresh_token")
    {
        return Err(ApiError::bad_request(
            "invalid_client_metadata",
            "Only authorization_code and refresh_token grants are supported",
        ));
    }
    if !request.response_types.is_empty()
        && request.response_types.iter().any(|value| value != "code")
    {
        return Err(ApiError::bad_request(
            "invalid_client_metadata",
            "Only the code response type is supported",
        ));
    }
    let scopes = normalize_registered_scopes(request.scope.as_deref())
        .map_err(|error| ApiError::bad_request("invalid_client_metadata", error.to_string()))?;
    let client_id = format!("dcr_{}", random_token(24));
    sqlx::query("INSERT INTO oauth_clients (client_id,client_name,client_uri,registration_type,redirect_uris,scopes,token_endpoint_auth_method,metadata) VALUES ($1,$2,$3,'dynamic',$4,$5,'none',$6)")
        .bind(&client_id)
        .bind(client_name)
        .bind(request.client_uri.as_deref())
        .bind(&request.redirect_uris)
        .bind(&scopes)
        .bind(json!({"application_type": "native-or-web"}))
        .execute(&state.db)
        .await
        .map_err(ApiError::internal)?;
    Ok((
        StatusCode::CREATED,
        Json(RegistrationResponse {
            client_id,
            client_name: client_name.to_string(),
            client_uri: request.client_uri,
            redirect_uris: request.redirect_uris,
            grant_types: vec!["authorization_code", "refresh_token"],
            response_types: vec!["code"],
            token_endpoint_auth_method: "none",
            scope: scopes.join(" "),
            client_id_issued_at: Utc::now().timestamp(),
        }),
    ))
}

#[derive(Debug, Deserialize)]
struct AuthorizeQuery {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    scope: Option<String>,
    resource: Option<String>,
    state: Option<String>,
    nonce: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
}

async fn authorize(
    State(state): State<AppState>,
    Query(query): Query<AuthorizeQuery>,
) -> ApiResult<Redirect> {
    if query.response_type != "code" {
        return Err(ApiError::bad_request(
            "unsupported_response_type",
            "Only response_type=code is supported",
        ));
    }
    let client = resolve_client(&state, &query.client_id).await?;
    if !client
        .redirect_uris
        .iter()
        .any(|value| redirect_uri_matches(value, &query.redirect_uri))
    {
        return Err(ApiError::bad_request(
            "invalid_redirect_uri",
            "redirect_uri is not registered for this client",
        ));
    }
    validate_redirect_uri(&query.redirect_uri)
        .map_err(|error| ApiError::bad_request("invalid_redirect_uri", error.to_string()))?;
    let resource = query.resource.as_deref().ok_or_else(|| {
        ApiError::bad_request("invalid_target", "The resource parameter is required")
    })?;
    if !resource_matches(resource, &state.config.mcp_resource) {
        return Err(ApiError::bad_request(
            "invalid_target",
            "The resource does not match this MCP server",
        ));
    }
    let scopes = normalize_scopes(query.scope.as_deref())
        .map_err(|error| ApiError::bad_request("invalid_scope", error.to_string()))?;
    if scopes
        .iter()
        .any(|scope| !client.scopes.iter().any(|allowed| allowed == scope))
    {
        return Err(ApiError::bad_request(
            "invalid_scope",
            "The client is not registered for every requested scope",
        ));
    }
    let challenge = query
        .code_challenge
        .as_deref()
        .filter(|value| (43..=128).contains(&value.len()))
        .ok_or_else(|| {
            ApiError::bad_request("invalid_request", "A valid PKCE code_challenge is required")
        })?;
    if query.code_challenge_method.as_deref() != Some("S256") {
        return Err(ApiError::bad_request(
            "invalid_request",
            "code_challenge_method must be S256",
        ));
    }
    let request_id = Uuid::new_v4();
    let expires_at = Utc::now()
        + chrono::Duration::from_std(state.config.auth_request_ttl).map_err(ApiError::internal)?;
    sqlx::query("INSERT INTO oauth_authorization_requests (request_id,client_id,redirect_uri,scopes,resource,state,nonce,code_challenge,code_challenge_method,expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'S256',$9)")
        .bind(request_id)
        .bind(&client.client_id)
        .bind(&query.redirect_uri)
        .bind(&scopes)
        .bind(state.config.mcp_resource.as_str())
        .bind(query.state.as_deref())
        .bind(query.nonce.as_deref())
        .bind(challenge)
        .bind(expires_at)
        .execute(&state.db)
        .await
        .map_err(ApiError::internal)?;
    let mut consent = state
        .config
        .app_url
        .join("/oauth/consent")
        .map_err(ApiError::internal)?;
    consent
        .query_pairs_mut()
        .append_pair("request_id", &request_id.to_string());
    Ok(Redirect::temporary(consent.as_str()))
}

#[derive(Debug)]
struct Client {
    client_id: String,
    redirect_uris: Vec<String>,
    scopes: Vec<String>,
    registration_type: String,
    status: String,
}

async fn resolve_client(state: &AppState, client_id: &str) -> ApiResult<Client> {
    if let Some(client) = load_client(state, client_id).await? {
        if client.status != "active" {
            return Err(ApiError::bad_request(
                "invalid_client",
                "Client registration is disabled",
            ));
        }
        if client.registration_type != "cimd" {
            return Ok(client);
        }
    }
    let metadata_url = validate_client_metadata_url(client_id, &state.config.cimd_allowed_origins)
        .map_err(|error| ApiError::bad_request("invalid_client", error.to_string()))?;
    let addresses = public_addresses(&metadata_url).await?;
    let host = metadata_url.host_str().ok_or_else(|| {
        ApiError::bad_request("invalid_client", "Client metadata URL has no host")
    })?;
    let client = reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(8))
        .user_agent("agent-saas-starter-cimd/0.1")
        .resolve_to_addrs(host, &addresses)
        .build()
        .map_err(ApiError::internal)?;
    let mut response = client
        .get(metadata_url.clone())
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(ApiError::internal)?
        .error_for_status()
        .map_err(|_| {
            ApiError::bad_request("invalid_client", "Client metadata could not be fetched")
        })?;
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !content_type.starts_with("application/json") {
        return Err(ApiError::bad_request(
            "invalid_client",
            "Client metadata must use application/json",
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > CIMD_MAX_BYTES as u64)
    {
        return Err(ApiError::bad_request(
            "invalid_client",
            "Client metadata is too large",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(ApiError::internal)? {
        if bytes.len().saturating_add(chunk.len()) > CIMD_MAX_BYTES {
            return Err(ApiError::bad_request(
                "invalid_client",
                "Client metadata is too large",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let metadata = serde_json::from_slice::<CimdClientMetadata>(&bytes)
        .map_err(|_| ApiError::bad_request("invalid_client", "Client metadata is invalid"))?;
    if metadata.client_id != client_id {
        return Err(ApiError::bad_request(
            "invalid_client",
            "Client metadata client_id does not match its URL",
        ));
    }
    choose_token_auth_method(&metadata)
        .map_err(|error| ApiError::bad_request("invalid_client", error.to_string()))?;
    if metadata.client_name.trim().is_empty()
        || metadata.client_name.chars().count() > 120
        || metadata.redirect_uris.is_empty()
        || metadata.redirect_uris.len() > 10
    {
        return Err(ApiError::bad_request(
            "invalid_client",
            "Client metadata needs a valid name and between 1 and 10 redirect URIs",
        ));
    }
    for redirect in &metadata.redirect_uris {
        validate_cimd_redirect_uri(&metadata_url, redirect)
            .map_err(|error| ApiError::bad_request("invalid_client", error.to_string()))?;
    }
    if metadata
        .grant_types
        .iter()
        .any(|value| value != "authorization_code" && value != "refresh_token")
        || metadata.response_types.iter().any(|value| value != "code")
    {
        return Err(ApiError::bad_request(
            "invalid_client",
            "Client metadata requests an unsupported OAuth flow",
        ));
    }
    let scopes = normalize_registered_scopes(metadata.scope.as_deref())
        .map_err(|error| ApiError::bad_request("invalid_client", error.to_string()))?;
    sqlx::query("INSERT INTO oauth_clients (client_id,client_name,client_uri,registration_type,redirect_uris,scopes,token_endpoint_auth_method,metadata) VALUES ($1,$2,$3,'cimd',$4,$5,'none',$6) ON CONFLICT (client_id) DO UPDATE SET client_name=EXCLUDED.client_name,client_uri=EXCLUDED.client_uri,redirect_uris=EXCLUDED.redirect_uris,scopes=EXCLUDED.scopes,metadata=EXCLUDED.metadata,updated_at=now()")
        .bind(client_id)
        .bind(metadata.client_name.trim())
        .bind(metadata.client_uri.as_deref())
        .bind(&metadata.redirect_uris)
        .bind(&scopes)
        .bind(serde_json::to_value(&metadata).map_err(ApiError::internal)?)
        .execute(&state.db)
        .await
        .map_err(ApiError::internal)?;
    Ok(Client {
        client_id: client_id.to_string(),
        redirect_uris: metadata.redirect_uris,
        scopes,
        registration_type: "cimd".to_string(),
        status: "active".to_string(),
    })
}

async fn load_client(state: &AppState, client_id: &str) -> ApiResult<Option<Client>> {
    let row = sqlx::query("SELECT client_id,registration_type,redirect_uris,scopes,status FROM oauth_clients WHERE client_id=$1")
        .bind(client_id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::internal)?;
    Ok(row.map(|row| Client {
        client_id: row.get("client_id"),
        redirect_uris: row.get("redirect_uris"),
        scopes: row.get("scopes"),
        registration_type: row.get("registration_type"),
        status: row.get("status"),
    }))
}

async fn public_addresses(url: &Url) -> ApiResult<Vec<SocketAddr>> {
    let host = url.host_str().ok_or_else(|| {
        ApiError::bad_request("invalid_client", "Client metadata URL has no host")
    })?;
    let port = url.port_or_known_default().unwrap_or(443);
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| {
            ApiError::bad_request("invalid_client", "Client metadata host cannot be resolved")
        })?
        .collect::<Vec<_>>();
    if addresses.is_empty() || addresses.iter().any(|address| !public_ip(address.ip())) {
        return Err(ApiError::bad_request(
            "invalid_client",
            "Client metadata host resolves to a private or reserved address",
        ));
    }
    Ok(addresses)
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_unspecified()
                || ip.octets()[0] == 0
                || ip.octets()[0] >= 240)
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(mapped));
            }
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || (ip.segments()[0] == 0x2001 && ip.segments()[1] == 0x0db8))
        }
    }
}

fn resource_matches(requested: &str, configured: &Url) -> bool {
    Url::parse(requested).is_ok_and(|requested| requested == *configured)
}

#[derive(Debug, Serialize)]
struct ConsentView {
    request_id: Uuid,
    client_id: String,
    client_name: String,
    client_uri: Option<String>,
    registration_type: String,
    redirect_uri: String,
    scopes: Vec<String>,
    resource: String,
    projects: Vec<ConsentProject>,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ConsentProject {
    workspace_id: Uuid,
    workspace_name: String,
    project_id: Uuid,
    project_name: String,
}

async fn consent_request(
    State(state): State<AppState>,
    axum::extract::Path(request_id): axum::extract::Path<Uuid>,
    headers: HeaderMap,
) -> ApiResult<Json<ConsentView>> {
    let session = state.session_user(&headers).await?;
    let row = sqlx::query("SELECT r.request_id,r.redirect_uri,r.scopes,r.resource,r.expires_at,c.client_id,c.client_name,c.client_uri,c.registration_type FROM oauth_authorization_requests r JOIN oauth_clients c ON c.client_id=r.client_id WHERE r.request_id=$1 AND r.status='pending' AND r.expires_at > now()")
        .bind(request_id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Authorization request is unavailable or expired"))?;
    let projects = sqlx::query("SELECT w.workspace_id,w.name AS workspace_name,p.project_id,p.name AS project_name FROM memberships m JOIN workspaces w ON w.workspace_id=m.workspace_id JOIN projects p ON p.workspace_id=w.workspace_id WHERE m.user_id=$1 ORDER BY w.created_at,p.created_at")
        .bind(session.user_id)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .map(|row| ConsentProject {
            workspace_id: row.get("workspace_id"),
            workspace_name: row.get("workspace_name"),
            project_id: row.get("project_id"),
            project_name: row.get("project_name"),
        })
        .collect();
    Ok(Json(ConsentView {
        request_id,
        client_id: row.get("client_id"),
        client_name: row.get("client_name"),
        client_uri: row.get("client_uri"),
        registration_type: row.get("registration_type"),
        redirect_uri: row.get("redirect_uri"),
        scopes: row.get("scopes"),
        resource: row.get("resource"),
        projects,
        expires_at: row.get("expires_at"),
    }))
}

#[derive(Debug, Deserialize)]
struct ConsentDecision {
    approve: bool,
    workspace_id: Option<Uuid>,
    project_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
struct ConsentDecisionResponse {
    redirect_to: String,
}

async fn decide_consent(
    State(state): State<AppState>,
    axum::extract::Path(request_id): axum::extract::Path<Uuid>,
    headers: HeaderMap,
    Json(decision): Json<ConsentDecision>,
) -> ApiResult<Json<ConsentDecisionResponse>> {
    let session = state.session_user(&headers).await?;
    let mut transaction = state.db.begin().await.map_err(ApiError::internal)?;
    let row = sqlx::query("SELECT client_id,redirect_uri,scopes,resource,state,nonce,code_challenge,expires_at FROM oauth_authorization_requests WHERE request_id=$1 AND status='pending' AND expires_at > now() FOR UPDATE")
        .bind(request_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Authorization request is unavailable or expired"))?;
    let redirect_uri: String = row.get("redirect_uri");
    let oauth_state: Option<String> = row.get("state");
    let issuer = state.config.auth_issuer.as_str().trim_end_matches('/');
    if !decision.approve {
        sqlx::query("UPDATE oauth_authorization_requests SET status='denied',user_id=$1,decided_at=now() WHERE request_id=$2")
            .bind(session.user_id)
            .bind(request_id)
            .execute(&mut *transaction)
            .await
            .map_err(ApiError::internal)?;
        transaction.commit().await.map_err(ApiError::internal)?;
        let redirect_to = oauth_error_redirect(
            &redirect_uri,
            &OAuthErrorRedirect {
                error: "access_denied".to_string(),
                description: "The user denied this authorization request".to_string(),
            },
            oauth_state.as_deref(),
            issuer,
        )
        .map_err(ApiError::internal)?;
        return Ok(Json(ConsentDecisionResponse { redirect_to }));
    }
    let workspace_id = decision.workspace_id.ok_or_else(|| {
        ApiError::bad_request("invalid_selection", "Select a workspace and project")
    })?;
    let project_id = decision.project_id.ok_or_else(|| {
        ApiError::bad_request("invalid_selection", "Select a workspace and project")
    })?;
    let role = sqlx::query_scalar::<_, String>("SELECT m.role FROM memberships m JOIN projects p ON p.workspace_id=m.workspace_id WHERE m.user_id=$1 AND m.workspace_id=$2 AND p.project_id=$3")
        .bind(session.user_id)
        .bind(workspace_id)
        .bind(project_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::forbidden("The selected project is not available to this user"))?;
    let grant_id = Uuid::new_v4();
    let scopes: Vec<String> = row.get("scopes");
    let client_id: String = row.get("client_id");
    let resource: String = row.get("resource");
    sqlx::query("INSERT INTO oauth_grants (grant_id,client_id,user_id,workspace_id,project_id,scopes,resource,auth_time) VALUES ($1,$2,$3,$4,$5,$6,$7,now())")
        .bind(grant_id)
        .bind(&client_id)
        .bind(session.user_id)
        .bind(workspace_id)
        .bind(project_id)
        .bind(&scopes)
        .bind(&resource)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    let code = random_token(32);
    let code_expiry = Utc::now()
        + chrono::Duration::from_std(state.config.auth_code_ttl).map_err(ApiError::internal)?;
    sqlx::query("INSERT INTO oauth_authorization_codes (code_hash,grant_id,redirect_uri,nonce,code_challenge,expires_at) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(hash_token(&code))
        .bind(grant_id)
        .bind(&redirect_uri)
        .bind(row.get::<Option<String>, _>("nonce"))
        .bind(row.get::<String, _>("code_challenge"))
        .bind(code_expiry)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    sqlx::query("UPDATE oauth_authorization_requests SET status='approved',user_id=$1,workspace_id=$2,project_id=$3,decided_at=now() WHERE request_id=$4")
        .bind(session.user_id)
        .bind(workspace_id)
        .bind(project_id)
        .bind(request_id)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    insert_audit(
        &mut transaction,
        session.user_id,
        workspace_id,
        "oauth.grant.approved",
        &client_id,
        json!({"project_id": project_id, "scopes": scopes, "role": role}),
    )
    .await?;
    transaction.commit().await.map_err(ApiError::internal)?;
    let redirect_to = authorization_redirect(&redirect_uri, &code, oauth_state.as_deref(), issuer)
        .map_err(ApiError::internal)?;
    Ok(Json(ConsentDecisionResponse { redirect_to }))
}

#[derive(Debug)]
struct OAuthEndpointError {
    status: StatusCode,
    code: &'static str,
    description: String,
}

impl OAuthEndpointError {
    fn invalid_grant(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_grant",
            description: message.into(),
        }
    }

    fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            description: message.into(),
        }
    }

    fn server(error: impl std::fmt::Display) -> Self {
        tracing::error!(error = %error, "OAuth endpoint failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "server_error",
            description: "The authorization server could not complete the request".to_string(),
        }
    }
}

impl IntoResponse for OAuthEndpointError {
    fn into_response(self) -> Response {
        (
            self.status,
            [
                (header::CACHE_CONTROL, "no-store"),
                (header::PRAGMA, "no-cache"),
            ],
            Json(json!({"error": self.code, "error_description": self.description})),
        )
            .into_response()
    }
}

async fn token(
    State(state): State<AppState>,
    Form(request): Form<OAuthTokenRequest>,
) -> Result<Response, OAuthEndpointError> {
    match request.grant_type.as_str() {
        "authorization_code" => authorization_code_token(&state, &request).await,
        "refresh_token" => refresh_token(&state, &request).await,
        _ => Err(OAuthEndpointError {
            status: StatusCode::BAD_REQUEST,
            code: "unsupported_grant_type",
            description: "Only authorization_code and refresh_token are supported".to_string(),
        }),
    }
}

async fn authorization_code_token(
    state: &AppState,
    request: &OAuthTokenRequest,
) -> Result<Response, OAuthEndpointError> {
    let client_id = required_field(request.client_id.as_deref(), "client_id")?;
    let code = required_field(request.code.as_deref(), "code")?;
    let redirect_uri = required_field(request.redirect_uri.as_deref(), "redirect_uri")?;
    let verifier = required_field(request.code_verifier.as_deref(), "code_verifier")?;
    let resource = required_field(request.resource.as_deref(), "resource")?;
    let mut transaction = state.db.begin().await.map_err(OAuthEndpointError::server)?;
    let row = sqlx::query("SELECT c.grant_id,c.redirect_uri,c.nonce,c.code_challenge,c.expires_at,c.consumed_at,g.client_id,g.user_id,g.workspace_id,g.project_id,g.scopes,g.resource,g.auth_time,g.revoked_at,u.email,u.email_verified_at,u.name,m.role FROM oauth_authorization_codes c JOIN oauth_grants g ON g.grant_id=c.grant_id JOIN users u ON u.user_id=g.user_id JOIN memberships m ON m.user_id=g.user_id AND m.workspace_id=g.workspace_id WHERE c.code_hash=$1 FOR UPDATE")
        .bind(hash_token(code))
        .fetch_optional(&mut *transaction)
        .await
        .map_err(OAuthEndpointError::server)?
        .ok_or_else(|| OAuthEndpointError::invalid_grant("Authorization code is invalid"))?;
    if row.get::<Option<DateTime<Utc>>, _>("consumed_at").is_some()
        || row.get::<DateTime<Utc>, _>("expires_at") <= Utc::now()
        || row.get::<Option<DateTime<Utc>>, _>("revoked_at").is_some()
        || row.get::<String, _>("client_id") != client_id
        || row.get::<String, _>("redirect_uri") != redirect_uri
        || !resource_matches(resource, &state.config.mcp_resource)
        || !resource_matches(
            &row.get::<String, _>("resource"),
            &state.config.mcp_resource,
        )
        || !verify_pkce(verifier, &row.get::<String, _>("code_challenge"))
    {
        return Err(OAuthEndpointError::invalid_grant(
            "Authorization code validation failed",
        ));
    }
    sqlx::query("UPDATE oauth_authorization_codes SET consumed_at=now() WHERE code_hash=$1")
        .bind(hash_token(code))
        .execute(&mut *transaction)
        .await
        .map_err(OAuthEndpointError::server)?;
    let issued = issue_tokens(state, &mut transaction, &row, None).await?;
    transaction
        .commit()
        .await
        .map_err(OAuthEndpointError::server)?;
    Ok(token_response(issued))
}

async fn refresh_token(
    state: &AppState,
    request: &OAuthTokenRequest,
) -> Result<Response, OAuthEndpointError> {
    let client_id = required_field(request.client_id.as_deref(), "client_id")?;
    let refresh = required_field(request.refresh_token.as_deref(), "refresh_token")?;
    let resource = required_field(request.resource.as_deref(), "resource")?;
    let mut transaction = state.db.begin().await.map_err(OAuthEndpointError::server)?;
    let row = sqlx::query("SELECT r.refresh_token_id,r.family_id,r.expires_at,r.consumed_at,r.revoked_at,g.grant_id,g.client_id,g.user_id,g.workspace_id,g.project_id,g.scopes,g.resource,g.auth_time,g.revoked_at AS grant_revoked_at,u.email,u.email_verified_at,u.name,m.role FROM oauth_refresh_tokens r JOIN oauth_grants g ON g.grant_id=r.grant_id JOIN users u ON u.user_id=g.user_id JOIN memberships m ON m.user_id=g.user_id AND m.workspace_id=g.workspace_id WHERE r.token_hash=$1 FOR UPDATE")
        .bind(hash_token(refresh))
        .fetch_optional(&mut *transaction)
        .await
        .map_err(OAuthEndpointError::server)?
        .ok_or_else(|| OAuthEndpointError::invalid_grant("Refresh token is invalid"))?;
    let family_id: Uuid = row.get("family_id");
    if row.get::<Option<DateTime<Utc>>, _>("consumed_at").is_some() {
        sqlx::query("UPDATE oauth_refresh_tokens SET revoked_at=COALESCE(revoked_at,now()) WHERE family_id=$1")
            .bind(family_id)
            .execute(&mut *transaction)
            .await
            .map_err(OAuthEndpointError::server)?;
        sqlx::query(
            "UPDATE oauth_grants SET revoked_at=COALESCE(revoked_at,now()) WHERE grant_id=$1",
        )
        .bind(row.get::<Uuid, _>("grant_id"))
        .execute(&mut *transaction)
        .await
        .map_err(OAuthEndpointError::server)?;
        transaction
            .commit()
            .await
            .map_err(OAuthEndpointError::server)?;
        return Err(OAuthEndpointError::invalid_grant(
            "Refresh token reuse was detected; reconnect the client",
        ));
    }
    if row.get::<DateTime<Utc>, _>("expires_at") <= Utc::now()
        || row.get::<Option<DateTime<Utc>>, _>("revoked_at").is_some()
        || row
            .get::<Option<DateTime<Utc>>, _>("grant_revoked_at")
            .is_some()
        || row.get::<String, _>("client_id") != client_id
        || !resource_matches(resource, &state.config.mcp_resource)
        || !resource_matches(
            &row.get::<String, _>("resource"),
            &state.config.mcp_resource,
        )
    {
        return Err(OAuthEndpointError::invalid_grant(
            "Refresh token validation failed",
        ));
    }
    let granted_scopes: Vec<String> = row.get("scopes");
    let requested_scopes = if let Some(scope) = request.scope.as_deref() {
        let scopes = normalize_scopes(Some(scope))
            .map_err(|error| OAuthEndpointError::invalid_request(error.to_string()))?;
        if scopes
            .iter()
            .any(|scope| !granted_scopes.iter().any(|granted| granted == scope))
        {
            return Err(OAuthEndpointError::invalid_request(
                "Refresh scope cannot exceed the original grant",
            ));
        }
        Some(scopes)
    } else {
        None
    };
    let current_id: Uuid = row.get("refresh_token_id");
    sqlx::query("UPDATE oauth_refresh_tokens SET consumed_at=now() WHERE refresh_token_id=$1")
        .bind(current_id)
        .execute(&mut *transaction)
        .await
        .map_err(OAuthEndpointError::server)?;
    let issued = issue_tokens(state, &mut transaction, &row, requested_scopes).await?;
    if let Some(replacement_id) = issued.refresh_token_id {
        sqlx::query("UPDATE oauth_refresh_tokens SET replaced_by=$1 WHERE refresh_token_id=$2")
            .bind(replacement_id)
            .bind(current_id)
            .execute(&mut *transaction)
            .await
            .map_err(OAuthEndpointError::server)?;
    }
    transaction
        .commit()
        .await
        .map_err(OAuthEndpointError::server)?;
    Ok(token_response(issued))
}

struct IssuedTokens {
    response: OAuthTokenResponse,
    refresh_token_id: Option<Uuid>,
}

async fn issue_tokens(
    state: &AppState,
    transaction: &mut Transaction<'_, Postgres>,
    row: &sqlx::postgres::PgRow,
    narrowed_scopes: Option<Vec<String>>,
) -> Result<IssuedTokens, OAuthEndpointError> {
    let scopes = narrowed_scopes.unwrap_or_else(|| row.get("scopes"));
    let issued_at = Utc::now().timestamp() as u64;
    let expires_at = issued_at + state.config.access_token_ttl.as_secs();
    let user_id: Uuid = row.get("user_id");
    let workspace_id: Uuid = row.get("workspace_id");
    let project_id: Uuid = row.get("project_id");
    let client_id: String = row.get("client_id");
    let resource: String = row.get("resource");
    let role: String = row.get("role");
    let context = AuthContext {
        user_id,
        workspace_id,
        project_id,
        role: role.clone(),
        scopes: scopes.clone(),
        client_id: client_id.clone(),
        token_id: Uuid::new_v4(),
    };
    let access_token = state
        .jwt
        .sign_access_token(&context, &resource, issued_at, expires_at)
        .map_err(OAuthEndpointError::server)?;
    let id_token = if scopes.iter().any(|scope| scope == "openid") {
        let auth_time: DateTime<Utc> = row.get("auth_time");
        Some(
            state
                .jwt
                .sign_id_token(&IdTokenClaims {
                    iss: state.jwt.issuer().to_string(),
                    sub: user_id.to_string(),
                    aud: client_id.clone(),
                    exp: expires_at,
                    iat: issued_at,
                    auth_time: auth_time.timestamp() as u64,
                    nonce: row.try_get("nonce").ok(),
                    email: scopes
                        .iter()
                        .any(|scope| scope == "email")
                        .then(|| row.get("email")),
                    email_verified: scopes.iter().any(|scope| scope == "email").then(|| {
                        row.get::<Option<DateTime<Utc>>, _>("email_verified_at")
                            .is_some()
                    }),
                    name: scopes
                        .iter()
                        .any(|scope| scope == "profile")
                        .then(|| row.get("name")),
                    workspace_id: scopes
                        .iter()
                        .any(|scope| scope == "project:read")
                        .then_some(workspace_id),
                    project_id: scopes
                        .iter()
                        .any(|scope| scope == "project:read")
                        .then_some(project_id),
                    role: scopes
                        .iter()
                        .any(|scope| scope == "project:read")
                        .then_some(role),
                })
                .map_err(OAuthEndpointError::server)?,
        )
    } else {
        None
    };
    let (refresh_token, refresh_token_id) = if scopes.iter().any(|scope| scope == "offline_access")
    {
        let token = random_token(32);
        let token_id = Uuid::new_v4();
        let family_id = row
            .try_get::<Uuid, _>("family_id")
            .unwrap_or_else(|_| Uuid::new_v4());
        let parent_id = row.try_get::<Uuid, _>("refresh_token_id").ok();
        let refresh_expiry = Utc::now()
            + chrono::Duration::from_std(state.config.refresh_token_ttl)
                .map_err(OAuthEndpointError::server)?;
        sqlx::query("INSERT INTO oauth_refresh_tokens (refresh_token_id,token_hash,grant_id,family_id,parent_id,expires_at) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(token_id)
            .bind(hash_token(&token))
            .bind(row.get::<Uuid, _>("grant_id"))
            .bind(family_id)
            .bind(parent_id)
            .bind(refresh_expiry)
            .execute(&mut **transaction)
            .await
            .map_err(OAuthEndpointError::server)?;
        (Some(token), Some(token_id))
    } else {
        (None, None)
    };
    Ok(IssuedTokens {
        response: OAuthTokenResponse {
            access_token,
            token_type: "Bearer",
            expires_in: state.config.access_token_ttl.as_secs(),
            scope: scopes.join(" "),
            refresh_token,
            id_token,
        },
        refresh_token_id,
    })
}

fn token_response(issued: IssuedTokens) -> Response {
    (
        StatusCode::OK,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        Json(issued.response),
    )
        .into_response()
}

fn required_field<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str, OAuthEndpointError> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| OAuthEndpointError::invalid_request(format!("{name} is required")))
}

#[derive(Debug, Deserialize)]
struct RevokeRequest {
    token: String,
    client_id: Option<String>,
    token_type_hint: Option<String>,
}

async fn revoke(
    State(state): State<AppState>,
    Form(request): Form<RevokeRequest>,
) -> Result<StatusCode, OAuthEndpointError> {
    let _ = request.token_type_hint;
    let client_id = required_field(request.client_id.as_deref(), "client_id")?;
    let token_hash = hash_token(request.token.trim());
    let family = sqlx::query_scalar::<_, Uuid>("SELECT r.family_id FROM oauth_refresh_tokens r JOIN oauth_grants g ON g.grant_id=r.grant_id WHERE r.token_hash=$1 AND g.client_id=$2")
        .bind(token_hash)
        .bind(client_id)
        .fetch_optional(&state.db)
        .await
        .map_err(OAuthEndpointError::server)?;
    if let Some(family_id) = family {
        sqlx::query("UPDATE oauth_refresh_tokens SET revoked_at=COALESCE(revoked_at,now()) WHERE family_id=$1")
            .bind(family_id)
            .execute(&state.db)
            .await
            .map_err(OAuthEndpointError::server)?;
    }
    Ok(StatusCode::OK)
}

async fn userinfo(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, OAuthEndpointError> {
    let token = bearer_token(&headers)
        .ok_or_else(|| OAuthEndpointError::invalid_request("Bearer access token is required"))?;
    let context = state
        .jwt
        .verifier()
        .verify_access_token(token, state.config.mcp_resource.as_str(), &[])
        .map_err(|_| OAuthEndpointError {
            status: StatusCode::UNAUTHORIZED,
            code: "invalid_token",
            description: "Access token is invalid or expired".to_string(),
        })?;
    if !context.scopes.iter().any(|scope| scope == "openid") {
        return Err(OAuthEndpointError {
            status: StatusCode::FORBIDDEN,
            code: "insufficient_scope",
            description: "The openid scope is required for UserInfo".to_string(),
        });
    }
    let row = sqlx::query("SELECT email,email_verified_at,name FROM users WHERE user_id=$1")
        .bind(context.user_id)
        .fetch_one(&state.db)
        .await
        .map_err(OAuthEndpointError::server)?;
    let mut claims = serde_json::Map::new();
    claims.insert("sub".to_string(), json!(context.user_id));
    if context.scopes.iter().any(|scope| scope == "email") {
        claims.insert("email".to_string(), json!(row.get::<String, _>("email")));
        claims.insert(
            "email_verified".to_string(),
            json!(
                row.get::<Option<DateTime<Utc>>, _>("email_verified_at")
                    .is_some()
            ),
        );
    }
    if context.scopes.iter().any(|scope| scope == "profile") {
        claims.insert("name".to_string(), json!(row.get::<String, _>("name")));
    }
    if context.scopes.iter().any(|scope| scope == "project:read") {
        claims.insert("workspace_id".to_string(), json!(context.workspace_id));
        claims.insert("project_id".to_string(), json!(context.project_id));
        claims.insert("role".to_string(), json!(context.role));
    }
    Ok(Json(Value::Object(claims)))
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.trim().is_empty()).then_some(token.trim())
}

async fn insert_audit(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    workspace_id: Uuid,
    event_type: &str,
    target_id: &str,
    metadata: Value,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO audit_events (audit_event_id,actor_user_id,workspace_id,event_type,target_type,target_id,metadata) VALUES ($1,$2,$3,$4,'oauth_client',$5,$6)")
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(workspace_id)
        .bind(event_type)
        .bind(target_id)
        .bind(metadata)
        .execute(&mut **transaction)
        .await
        .map_err(ApiError::internal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_metadata_advertises_agent_client_requirements() {
        let metadata = authorization_server_metadata_document(
            "https://auth.example",
            "https://docs.example",
            "Example",
            true,
            true,
        );

        assert_eq!(metadata["issuer"], "https://auth.example");
        assert_eq!(
            metadata["authorization_endpoint"],
            "https://auth.example/oauth/authorize"
        );
        assert_eq!(
            metadata["code_challenge_methods_supported"],
            json!(["S256"])
        );
        assert_eq!(
            metadata["token_endpoint_auth_methods_supported"],
            json!(["none"])
        );
        assert_eq!(metadata["client_id_metadata_document_supported"], true);
        assert_eq!(
            metadata["registration_endpoint"],
            "https://auth.example/oauth/register"
        );
        assert_eq!(metadata["resource_parameter_supported"], true);
        assert_eq!(
            metadata["authorization_response_iss_parameter_supported"],
            true
        );
    }

    #[test]
    fn authorization_metadata_hides_disabled_registration() {
        let metadata = authorization_server_metadata_document(
            "https://auth.example",
            "https://docs.example",
            "Example",
            false,
            false,
        );

        assert!(metadata.get("registration_endpoint").is_none());
        assert_eq!(metadata["client_id_metadata_document_supported"], false);
    }

    #[test]
    fn private_and_reserved_addresses_are_rejected() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.1.1",
            "192.168.1.1",
            "169.254.1.1",
            "::1",
            "fc00::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "2001:db8::1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip} should be rejected");
        }
        assert!(public_ip("1.1.1.1".parse().unwrap()));
        assert!(public_ip("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn resource_matching_uses_url_canonicalization() {
        let resource = Url::parse("https://mcp.example.com/mcp").unwrap();
        assert!(resource_matches(
            "https://MCP.EXAMPLE.COM:443/mcp",
            &resource
        ));
        assert!(!resource_matches(
            "https://mcp.example.com/other",
            &resource
        ));
        assert!(!resource_matches(
            "https://mcp.example.com/mcp#fragment",
            &resource
        ));
    }
}
