use anyhow::{Context, bail};
use axum::{Json, Router, middleware, routing::get};
use rmcp::{
    ErrorData, ServerHandler,
    handler::server::{router::tool::ToolRouter, tool::Extension},
    model::{CallToolResult, MetaObject, ProtocolVersion},
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use starter_auth::JwtVerifier;
use starter_mcp::{
    McpAuthorizer, mirror_tool_security_schemes, protected_resource_metadata, require_project_read,
    tool_auth_error, tool_security_meta,
};
use std::{borrow::Cow, env, fs, net::SocketAddr, sync::Arc};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use url::Url;

#[derive(Clone)]
struct RandomServer {
    tool_router: ToolRouter<Self>,
    authorizer: Arc<McpAuthorizer>,
    http: reqwest::Client,
    api_url: Url,
}

const SUPPORTED_MCP_VERSIONS: &[ProtocolVersion] = &[
    ProtocolVersion::V_2025_03_26,
    ProtocolVersion::V_2025_06_18,
    ProtocolVersion::V_2025_11_25,
];

#[derive(Debug, Serialize, Deserialize)]
struct RandomNumber {
    number: u32,
}

#[tool_router(router = tool_router)]
impl RandomServer {
    fn new(authorizer: Arc<McpAuthorizer>, http: reqwest::Client, api_url: Url) -> Self {
        Self {
            tool_router: Self::tool_router(),
            authorizer,
            http,
            api_url,
        }
    }

    /// Fetch a random integer between 0 and 100 inclusive from the authenticated API.
    #[tool(
        name = "get_random_number",
        annotations(
            title = "Get random number",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        ),
        meta = "random_tool_meta()"
    )]
    async fn get_random_number(
        &self,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.authorizer.validate(&parts.headers, &["project:read"]) {
            Ok(_) => (),
            Err(_) => {
                let challenge = self.authorizer.challenge(
                    &["project:read"],
                    Some("invalid_token"),
                    Some("Reconnect this MCP server"),
                );
                return Ok(tool_auth_error(
                    &challenge,
                    "Authentication is required to get a random number",
                ));
            }
        };
        let authorization = parts
            .headers
            .get(http::header::AUTHORIZATION)
            .ok_or_else(|| ErrorData::internal_error("Authorization header is missing", None))?;
        // The API and MCP are two transports of the same OAuth resource.
        let response = self
            .http
            .get(self.api_url.clone())
            .header(http::header::AUTHORIZATION, authorization)
            .send()
            .await
            .map_err(|_| ErrorData::internal_error("The random-number API is unavailable", None))?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Ok(tool_auth_error(
                &self.authorizer.challenge(
                    &["project:read"],
                    Some("invalid_token"),
                    Some("Reconnect this MCP server"),
                ),
                "Reconnect to get a random number",
            ));
        }
        let number = response
            .error_for_status()
            .map_err(|_| {
                ErrorData::internal_error("The random-number API rejected the request", None)
            })?
            .json::<RandomNumber>()
            .await
            .map_err(|_| {
                ErrorData::internal_error(
                    "The random-number API returned an invalid response",
                    None,
                )
            })?;
        Ok(CallToolResult::structured(json!(number)))
    }
}

fn random_tool_meta() -> MetaObject {
    let mut meta = tool_security_meta(&["project:read"]);
    meta.0.insert(
        "openai/toolInvocation/invoking".to_string(),
        json!("Getting a random number"),
    );
    meta.0.insert(
        "openai/toolInvocation/invoked".to_string(),
        json!("Random number ready"),
    );
    meta
}

#[tool_handler(
    router = self.tool_router,
    name = "agent-saas-starter",
    version = "0.1.0",
    instructions = "Use get_random_number to fetch a random integer from the API. This is the only tool."
)]
impl ServerHandler for RandomServer {
    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(SUPPORTED_MCP_VERSIONS)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();
    let bind = value("MCP_BIND", "127.0.0.1:4001")
        .parse::<SocketAddr>()
        .context("MCP_BIND must be a socket address")?;
    let issuer = Url::parse(&value("AUTH_ISSUER", "http://localhost:4000"))
        .context("AUTH_ISSUER must be an absolute URL")?;
    let resource = Url::parse(&value("MCP_RESOURCE", "http://localhost:4001/mcp"))
        .context("MCP_RESOURCE must be an absolute URL")?;
    let docs_url = Url::parse(&value("DOCS_URL", "http://localhost:3003"))
        .context("DOCS_URL must be an absolute URL")?;
    let production = value("APP_ENV", "development") == "production";
    if production && (issuer.scheme() != "https" || resource.scheme() != "https") {
        bail!("AUTH_ISSUER and MCP_RESOURCE must use HTTPS in production");
    }
    let public_key_path = value("AUTH_PUBLIC_KEY_PATH", ".local/auth-public.pem");
    let public_pem = fs::read(&public_key_path)
        .with_context(|| format!("read public key at {public_key_path}"))?;
    let verifier =
        JwtVerifier::from_public_pem(issuer.as_str().trim_end_matches('/'), &public_pem)?;
    let metadata_url = resource_metadata_url(&resource);
    let authorizer = Arc::new(McpAuthorizer::new(
        verifier,
        resource.to_string(),
        metadata_url.to_string(),
    ));
    let mut allowed_hosts = vec![resource.host_str().unwrap_or_default().to_string()];
    if !production {
        allowed_hosts.extend(["localhost".into(), "127.0.0.1".into(), "::1".into()]);
    }
    let cancellation = CancellationToken::new();
    let server_authorizer = authorizer.clone();
    let api_origin = Url::parse(&value("API_INTERNAL_URL", "http://localhost:4000"))?;
    if !matches!(api_origin.scheme(), "http" | "https") || api_origin.host_str().is_none() {
        bail!("API_INTERNAL_URL must be an HTTP(S) URL");
    }
    let api_url = api_origin.join("/api/v1/random-number")?;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(8))
        .build()?;
    let service = StreamableHttpService::new(
        move || {
            Ok(RandomServer::new(
                server_authorizer.clone(),
                http.clone(),
                api_url.clone(),
            ))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(allowed_hosts)
            .with_cancellation_token(cancellation.child_token()),
    );
    let resource_documentation = docs_url.join("/docs/guides/mcp")?;
    let metadata = protected_resource_metadata(
        resource.to_string(),
        issuer.as_str().trim_end_matches('/').to_string(),
        resource_documentation.to_string(),
    );
    let metadata_for_suffix = metadata.clone();
    let mcp = Router::new()
        .nest_service("/mcp", service)
        .route_layer(middleware::from_fn(mirror_tool_security_schemes))
        .route_layer(middleware::from_fn_with_state(
            authorizer,
            require_project_read,
        ));
    let app = Router::new()
        .route("/healthz", get(|| async { Json(json!({"status": "ok"})) }))
        .route(
            "/.well-known/oauth-protected-resource",
            get(move || {
                let metadata = metadata.clone();
                async move { Json(metadata) }
            }),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(move || {
                let metadata = metadata_for_suffix.clone();
                async move { Json(metadata) }
            }),
        )
        .merge(mcp)
        .layer(tower_http::trace::TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(address = %bind, resource = %resource, "MCP server listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            cancellation.cancel();
        })
        .await?;
    Ok(())
}

fn value(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_string())
}

fn resource_metadata_url(resource: &Url) -> Url {
    let mut metadata_url = resource.clone();
    metadata_url.set_path(&format!(
        "/.well-known/oauth-protected-resource{}",
        resource.path().trim_end_matches('/')
    ));
    metadata_url.set_query(None);
    metadata_url.set_fragment(None);
    metadata_url
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "starter_mcp_server=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_resource_metadata_url_preserves_the_resource_path() {
        let resource = Url::parse("https://mcp.example/mcp").unwrap();
        assert_eq!(
            resource_metadata_url(&resource).as_str(),
            "https://mcp.example/.well-known/oauth-protected-resource/mcp"
        );
    }
}
