use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;
use url::Url;

pub const CIMD_MAX_BYTES: usize = 64 * 1024;
pub const SUPPORTED_SCOPES: &[&str] = &[
    "openid",
    "profile",
    "email",
    "offline_access",
    "project:read",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CimdClientMetadata {
    pub client_id: String,
    pub client_name: String,
    #[serde(default)]
    pub client_uri: Option<String>,
    pub redirect_uris: Vec<String>,
    #[serde(default)]
    pub grant_types: Vec<String>,
    #[serde(default)]
    pub response_types: Vec<String>,
    #[serde(default)]
    pub token_endpoint_auth_methods_supported: Vec<String>,
    #[serde(default)]
    pub token_endpoint_auth_method: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthClientRegistration {
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    #[serde(default)]
    pub grant_types: Vec<String>,
    #[serde(default)]
    pub response_types: Vec<String>,
    #[serde(default = "public_client_auth_method")]
    pub token_endpoint_auth_method: String,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub client_uri: Option<String>,
}

fn public_client_auth_method() -> String {
    "none".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthTokenRequest {
    pub grant_type: String,
    pub client_id: Option<String>,
    pub code: Option<String>,
    pub redirect_uri: Option<String>,
    pub code_verifier: Option<String>,
    pub refresh_token: Option<String>,
    pub resource: Option<String>,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: u64,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OAuthErrorRedirect {
    pub error: String,
    pub description: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OAuthValidationError {
    #[error("URL is invalid")]
    InvalidUrl,
    #[error("URL must use HTTPS")]
    HttpsRequired,
    #[error("URL must not include user information, query, or fragment")]
    UnsafeUrl,
    #[error("client metadata URL must include a non-root path")]
    MetadataPathRequired,
    #[error("client origin is not allowed")]
    OriginNotAllowed,
    #[error("redirect URI must use HTTPS or an HTTP loopback address")]
    InvalidRedirect,
    #[error("client metadata redirect URI must be same-origin or an HTTP loopback address")]
    CrossOriginRedirect,
    #[error("unsupported token endpoint authentication method")]
    UnsupportedAuthMethod,
    #[error("unsupported scope: {0}")]
    UnsupportedScope(String),
}

pub fn validate_client_metadata_url(
    value: &str,
    allowed_origins: &[String],
) -> Result<Url, OAuthValidationError> {
    let url = Url::parse(value).map_err(|_| OAuthValidationError::InvalidUrl)?;
    if url.scheme() != "https" {
        return Err(OAuthValidationError::HttpsRequired);
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(OAuthValidationError::UnsafeUrl);
    }
    if url.path().is_empty() || url.path() == "/" {
        return Err(OAuthValidationError::MetadataPathRequired);
    }
    let origin = url.origin().ascii_serialization();
    if !allowed_origins.iter().any(|allowed| allowed == &origin) {
        return Err(OAuthValidationError::OriginNotAllowed);
    }
    Ok(url)
}

pub fn validate_redirect_uri(value: &str) -> Result<Url, OAuthValidationError> {
    let url = Url::parse(value).map_err(|_| OAuthValidationError::InvalidUrl)?;
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(OAuthValidationError::UnsafeUrl);
    }
    let host = url.host_str().unwrap_or_default();
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
        || host == "[::1]"
        || host == "::1";
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(OAuthValidationError::InvalidRedirect);
    }
    Ok(url)
}

pub fn redirect_uri_matches(registered: &str, requested: &str) -> bool {
    let Ok(registered_url) = validate_redirect_uri(registered) else {
        return false;
    };
    let Ok(requested_url) = validate_redirect_uri(requested) else {
        return false;
    };
    if registered == requested {
        return true;
    }
    registered_url.scheme() == "http"
        && requested_url.scheme() == "http"
        && loopback_host(&registered_url)
        && loopback_host(&requested_url)
        && registered_url
            .host_str()
            .zip(requested_url.host_str())
            .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
        && registered_url.path() == requested_url.path()
        && registered_url.query() == requested_url.query()
}

pub fn validate_cimd_redirect_uri(
    metadata_url: &Url,
    redirect_uri: &str,
) -> Result<Url, OAuthValidationError> {
    let redirect = validate_redirect_uri(redirect_uri)?;
    if loopback_host(&redirect)
        || (redirect.scheme() == "https" && redirect.origin() == metadata_url.origin())
    {
        Ok(redirect)
    } else {
        Err(OAuthValidationError::CrossOriginRedirect)
    }
}

fn loopback_host(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host == "127.0.0.1"
            || host == "[::1]"
            || host == "::1"
    })
}

pub fn choose_token_auth_method(
    metadata: &CimdClientMetadata,
) -> Result<&'static str, OAuthValidationError> {
    let supports_none = metadata
        .token_endpoint_auth_methods_supported
        .iter()
        .any(|method| method == "none")
        || metadata.token_endpoint_auth_method.as_deref() == Some("none");
    supports_none
        .then_some("none")
        .ok_or(OAuthValidationError::UnsupportedAuthMethod)
}

pub fn normalize_scopes(value: Option<&str>) -> Result<Vec<String>, OAuthValidationError> {
    let requested = value.unwrap_or("openid profile email project:read");
    let mut scopes = BTreeSet::new();
    for scope in requested.split_ascii_whitespace() {
        if !SUPPORTED_SCOPES.contains(&scope) {
            return Err(OAuthValidationError::UnsupportedScope(scope.to_string()));
        }
        scopes.insert(scope.to_string());
    }
    Ok(scopes.into_iter().collect())
}

pub fn normalize_registered_scopes(
    value: Option<&str>,
) -> Result<Vec<String>, OAuthValidationError> {
    match value {
        Some(value) => normalize_scopes(Some(value)),
        None => Ok(SUPPORTED_SCOPES
            .iter()
            .map(|scope| (*scope).to_string())
            .collect()),
    }
}

pub fn authorization_redirect(
    redirect_uri: &str,
    code: &str,
    state: Option<&str>,
    issuer: &str,
) -> Result<String, OAuthValidationError> {
    let mut url = validate_redirect_uri(redirect_uri)?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("code", code);
        if let Some(state) = state {
            query.append_pair("state", state);
        }
        query.append_pair("iss", issuer);
    }
    Ok(url.into())
}

pub fn oauth_error_redirect(
    redirect_uri: &str,
    error: &OAuthErrorRedirect,
    state: Option<&str>,
    issuer: &str,
) -> Result<String, OAuthValidationError> {
    let mut url = validate_redirect_uri(redirect_uri)?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("error", &error.error);
        query.append_pair("error_description", &error.description);
        if let Some(state) = state {
            query.append_pair("state", state);
        }
        query.append_pair("iss", issuer);
    }
    Ok(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cimd_accepts_current_plural_and_legacy_singular_methods() {
        let mut metadata = CimdClientMetadata {
            client_id: "https://chatgpt.com/.well-known/oauth-client/example".into(),
            client_name: "ChatGPT".into(),
            client_uri: None,
            redirect_uris: vec!["https://chatgpt.com/connector_platform_oauth_redirect".into()],
            grant_types: vec![],
            response_types: vec![],
            token_endpoint_auth_methods_supported: vec!["none".into(), "private_key_jwt".into()],
            token_endpoint_auth_method: None,
            scope: None,
        };
        assert_eq!(choose_token_auth_method(&metadata), Ok("none"));
        metadata.token_endpoint_auth_methods_supported.clear();
        metadata.token_endpoint_auth_method = Some("none".into());
        assert_eq!(choose_token_auth_method(&metadata), Ok("none"));
    }

    #[test]
    fn metadata_url_is_origin_bounded() {
        let allowed = vec!["https://chatgpt.com".to_string()];
        assert!(
            validate_client_metadata_url(
                "https://chatgpt.com/.well-known/oauth-client/starter",
                &allowed
            )
            .is_ok()
        );
        assert_eq!(
            validate_client_metadata_url("https://example.com/client.json", &allowed),
            Err(OAuthValidationError::OriginNotAllowed)
        );
        assert!(validate_client_metadata_url("http://chatgpt.com/client.json", &allowed).is_err());
    }

    #[test]
    fn redirect_uri_allows_https_and_native_loopback_only() {
        assert!(validate_redirect_uri("https://chatgpt.com/oauth/callback").is_ok());
        assert!(validate_redirect_uri("http://localhost:49152/callback").is_ok());
        assert!(validate_redirect_uri("http://127.0.0.1:49152/callback").is_ok());
        assert!(validate_redirect_uri("http://example.com/callback").is_err());
    }

    #[test]
    fn loopback_redirect_matching_ignores_only_the_port() {
        assert!(redirect_uri_matches(
            "http://localhost/callback",
            "http://localhost:49152/callback"
        ));
        assert!(redirect_uri_matches(
            "http://127.0.0.1/callback?flow=mcp",
            "http://127.0.0.1:49152/callback?flow=mcp"
        ));
        assert!(!redirect_uri_matches(
            "http://localhost/callback",
            "http://127.0.0.1:49152/callback"
        ));
        assert!(!redirect_uri_matches(
            "https://example.com/callback",
            "https://example.com:8443/callback"
        ));
        assert!(!redirect_uri_matches(
            "https://example.com/callback",
            "https://EXAMPLE.com:443/callback"
        ));
    }

    #[test]
    fn cimd_redirects_are_same_origin_or_loopback() {
        let metadata = Url::parse("https://chatgpt.com/oauth/client.json").unwrap();
        assert!(validate_cimd_redirect_uri(&metadata, "https://chatgpt.com/callback").is_ok());
        assert!(validate_cimd_redirect_uri(&metadata, "http://localhost/callback").is_ok());
        assert!(validate_cimd_redirect_uri(&metadata, "https://evil.example/callback").is_err());
    }

    #[test]
    fn absent_registration_scope_allows_the_advertised_catalog() {
        assert_eq!(
            normalize_registered_scopes(None).unwrap(),
            SUPPORTED_SCOPES
                .iter()
                .map(|scope| (*scope).to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn redirects_always_identify_the_issuer() {
        let success = authorization_redirect(
            "https://client.example/callback",
            "code",
            Some("state"),
            "https://auth.example",
        )
        .unwrap();
        assert!(success.contains("iss=https%3A%2F%2Fauth.example"));
        let failure = oauth_error_redirect(
            "https://client.example/callback",
            &OAuthErrorRedirect {
                error: "access_denied".into(),
                description: "Denied".into(),
            },
            None,
            "https://auth.example",
        )
        .unwrap();
        assert!(failure.contains("iss=https%3A%2F%2Fauth.example"));
    }
}
