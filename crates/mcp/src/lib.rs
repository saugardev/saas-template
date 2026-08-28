use axum::{
    Json,
    body::{Body, to_bytes},
    extract::State,
    http::{HeaderMap, Method, Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use rmcp::model::MetaObject;
use serde::Serialize;
use serde_json::{Value, json};
use starter_auth::{AuthContext, JwtError, JwtVerifier};
use std::sync::Arc;
use thiserror::Error;

const MCP_RESPONSE_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct ProtectedResourceMetadata {
    pub resource: String,
    pub authorization_servers: Vec<String>,
    pub scopes_supported: Vec<String>,
    pub resource_documentation: String,
    pub bearer_methods_supported: Vec<&'static str>,
}

#[derive(Clone)]
pub struct McpAuthorizer {
    verifier: JwtVerifier,
    resource: String,
    metadata_url: String,
}

#[derive(Debug, Error)]
pub enum McpAuthError {
    #[error("bearer token is required")]
    Missing,
    #[error("bearer token is invalid")]
    Invalid(#[source] JwtError),
}

impl McpAuthorizer {
    pub fn new(verifier: JwtVerifier, resource: String, metadata_url: String) -> Self {
        Self {
            verifier,
            resource,
            metadata_url,
        }
    }

    pub fn validate(
        &self,
        headers: &HeaderMap,
        required_scopes: &[&str],
    ) -> Result<AuthContext, McpAuthError> {
        let token = bearer_token(headers).ok_or(McpAuthError::Missing)?;
        self.verifier
            .verify_access_token(token, &self.resource, required_scopes)
            .map_err(McpAuthError::Invalid)
    }

    pub fn challenge(
        &self,
        required_scopes: &[&str],
        error: Option<&str>,
        description: Option<&str>,
    ) -> String {
        bearer_challenge(&self.metadata_url, required_scopes, error, description)
    }
}

fn bearer_challenge(
    metadata_url: &str,
    required_scopes: &[&str],
    error: Option<&str>,
    description: Option<&str>,
) -> String {
    let mut value = format!(
        "Bearer resource_metadata=\"{}\", scope=\"{}\"",
        metadata_url,
        required_scopes.join(" ")
    );
    if let Some(error) = error {
        value.push_str(&format!(", error=\"{error}\""));
    }
    if let Some(description) = description {
        let safe = description.replace(['"', '\r', '\n'], " ");
        value.push_str(&format!(", error_description=\"{safe}\""));
    }
    value
}

pub fn protected_resource_metadata(
    resource: impl Into<String>,
    issuer: impl Into<String>,
    documentation: impl Into<String>,
) -> ProtectedResourceMetadata {
    ProtectedResourceMetadata {
        resource: resource.into(),
        authorization_servers: vec![issuer.into()],
        scopes_supported: vec!["project:read".to_string()],
        resource_documentation: documentation.into(),
        bearer_methods_supported: vec!["header"],
    }
}

pub fn tool_security_meta(scopes: &[&str]) -> MetaObject {
    let mut meta = MetaObject::new();
    meta.0.insert(
        "securitySchemes".to_string(),
        json!([{ "type": "oauth2", "scopes": scopes }]),
    );
    meta
}

pub fn tool_auth_error(challenge: &str, message: &str) -> rmcp::model::CallToolResult {
    let mut meta = MetaObject::new();
    meta.0
        .insert("mcp/www_authenticate".to_string(), json!([challenge]));
    rmcp::model::CallToolResult::error(vec![rmcp::model::ContentBlock::text(message)])
        .with_meta(Some(meta))
}

pub async fn require_project_read(
    State(authorizer): State<Arc<McpAuthorizer>>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    match authorizer.validate(request.headers(), &["project:read"]) {
        Ok(context) => {
            request.extensions_mut().insert(context);
            next.run(request).await
        }
        Err(error) => {
            let (status, oauth_error, description) = match error {
                McpAuthError::Missing => (
                    StatusCode::UNAUTHORIZED,
                    None,
                    "Sign in to use this MCP server",
                ),
                McpAuthError::Invalid(JwtError::MissingScope) => (
                    StatusCode::FORBIDDEN,
                    Some("insufficient_scope"),
                    "Grant the project:read scope",
                ),
                McpAuthError::Invalid(_) => (
                    StatusCode::UNAUTHORIZED,
                    Some("invalid_token"),
                    "Reconnect this MCP server",
                ),
            };
            let challenge = authorizer.challenge(&["project:read"], oauth_error, Some(description));
            let mut response = (
                status,
                Json(json!({
                    "error": {
                        "code": oauth_error.unwrap_or("authentication_required"),
                        "message": description
                    }
                })),
            )
                .into_response();
            if let Ok(value) = challenge.parse() {
                response
                    .headers_mut()
                    .insert(header::WWW_AUTHENTICATE, value);
            }
            response
        }
    }
}

pub async fn mirror_tool_security_schemes(request: Request<Body>, next: Next) -> Response {
    if request.method() != Method::POST {
        return next.run(request).await;
    }

    let (parts, body) = request.into_parts();
    let Ok(bytes) = to_bytes(body, MCP_RESPONSE_LIMIT).await else {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            "MCP request is too large for metadata compatibility handling",
        )
            .into_response();
    };
    let should_mirror = is_tools_list_request(&bytes);
    let response = next
        .run(Request::from_parts(parts, Body::from(bytes)))
        .await;
    if !should_mirror || !response.status().is_success() {
        return response;
    }

    mirror_tools_list_response(response).await
}

fn is_tools_list_request(body: &[u8]) -> bool {
    let Ok(message) = serde_json::from_slice::<Value>(body) else {
        return false;
    };
    match message {
        Value::Object(object) => object.get("method").and_then(Value::as_str) == Some("tools/list"),
        Value::Array(messages) => messages
            .iter()
            .any(|message| message.get("method").and_then(Value::as_str) == Some("tools/list")),
        _ => false,
    }
}

async fn mirror_tools_list_response(response: Response) -> Response {
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = to_bytes(body, MCP_RESPONSE_LIMIT).await else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "MCP tools/list response is too large for metadata compatibility handling",
        )
            .into_response();
    };
    let Some(content_type) = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    let rewritten = if content_type.starts_with("application/json") {
        enrich_json_response(&bytes)
    } else if content_type.starts_with("text/event-stream") {
        enrich_event_stream_response(&bytes)
    } else {
        None
    };
    let Some(rewritten) = rewritten else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(rewritten))
}

fn enrich_json_response(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut message = serde_json::from_slice::<Value>(bytes).ok()?;
    if !enrich_tools_list(&mut message) {
        return None;
    }
    serde_json::to_vec(&message).ok()
}

fn enrich_event_stream_response(bytes: &[u8]) -> Option<Vec<u8>> {
    let body = std::str::from_utf8(bytes).ok()?;
    let mut changed = false;
    let mut output = String::with_capacity(body.len());
    for line in body.split_inclusive('\n') {
        let Some(rest) = line.strip_prefix("data:") else {
            output.push_str(line);
            continue;
        };
        let ending = if rest.ends_with('\n') { "\n" } else { "" };
        let payload = rest.trim_end_matches('\n').trim_start();
        let Ok(mut message) = serde_json::from_str::<Value>(payload) else {
            output.push_str(line);
            continue;
        };
        if enrich_tools_list(&mut message) {
            changed = true;
            output.push_str("data: ");
            output.push_str(&message.to_string());
            output.push_str(ending);
        } else {
            output.push_str(line);
        }
    }
    changed.then(|| output.into_bytes())
}

pub fn enrich_tools_list(message: &mut Value) -> bool {
    let Some(tools) = message
        .get_mut("result")
        .and_then(|result| result.get_mut("tools"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    for tool in tools {
        let Some(tool) = tool.as_object_mut() else {
            continue;
        };
        let schemes = tool
            .get("_meta")
            .and_then(|meta| meta.get("securitySchemes"))
            .cloned();
        if !tool.contains_key("securitySchemes")
            && let Some(schemes) = schemes
        {
            tool.insert("securitySchemes".to_string(), schemes);
            changed = true;
        }
    }
    changed
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty() && token.len() <= 16 * 1024)
        .then_some(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_does_not_require_offline_access_at_the_resource() {
        let metadata = protected_resource_metadata(
            "https://mcp.example/mcp",
            "https://auth.example",
            "https://docs.example/mcp",
        );
        assert_eq!(metadata.scopes_supported, vec!["project:read"]);
    }

    #[test]
    fn challenge_names_resource_scope_and_oauth_error() {
        assert_eq!(
            bearer_challenge(
                "https://mcp.example/.well-known/oauth-protected-resource/mcp",
                &["project:read"],
                Some("insufficient_scope"),
                Some("Grant the \"project:read\" scope\nthen retry"),
            ),
            "Bearer resource_metadata=\"https://mcp.example/.well-known/oauth-protected-resource/mcp\", scope=\"project:read\", error=\"insufficient_scope\", error_description=\"Grant the  project:read  scope then retry\""
        );
    }

    #[test]
    fn tool_auth_challenge_uses_the_compatible_array_shape() {
        let result = tool_auth_error("Bearer error=\"invalid_token\"", "Sign in");
        assert_eq!(
            result.meta.expect("auth metadata").0["mcp/www_authenticate"],
            json!(["Bearer error=\"invalid_token\""])
        );
    }

    #[test]
    fn mirrors_compatibility_security_schemes() {
        let mut message = json!({
            "result": {
                "tools": [{
                    "name": "get_project_context",
                    "_meta": {"securitySchemes": [{"type": "oauth2", "scopes": ["project:read"]}]}
                }]
            }
        });
        assert!(enrich_tools_list(&mut message));
        assert_eq!(
            message["result"]["tools"][0]["securitySchemes"],
            message["result"]["tools"][0]["_meta"]["securitySchemes"]
        );
    }

    #[test]
    fn detects_only_tools_list_requests() {
        assert!(is_tools_list_request(
            br#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#
        ));
        assert!(!is_tools_list_request(
            br#"{"jsonrpc":"2.0","id":2,"method":"tools/call"}"#
        ));
    }

    #[test]
    fn mirrors_security_schemes_in_event_streams() {
        let body = b"event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[{\"name\":\"context\",\"_meta\":{\"securitySchemes\":[{\"type\":\"oauth2\",\"scopes\":[\"project:read\"]}]}}]}}\n\n";
        let rewritten = enrich_event_stream_response(body).expect("response is enriched");
        let text = String::from_utf8(rewritten).expect("response remains UTF-8");
        assert!(text.contains("\"securitySchemes\""));
    }
}
