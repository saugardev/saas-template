use anyhow::{Context, bail};
use starter_auth::OidcProviders;
use std::{env, net::SocketAddr, path::PathBuf, time::Duration};
use url::Url;

#[derive(Clone)]
pub struct Config {
    pub app_env: String,
    pub app_name: String,
    pub app_url: Url,
    pub docs_url: Url,
    pub api_public_url: Url,
    pub api_internal_url: Url,
    pub auth_issuer: Url,
    pub mcp_resource: Url,
    pub database_url: String,
    pub bind: SocketAddr,
    pub private_key_path: PathBuf,
    pub public_key_path: PathBuf,
    pub key_id: String,
    pub session_ttl: Duration,
    pub access_token_ttl: Duration,
    pub refresh_token_ttl: Duration,
    pub auth_code_ttl: Duration,
    pub auth_request_ttl: Duration,
    pub handoff_ttl: Duration,
    pub oidc_providers: OidcProviders,
    pub dcr_enabled: bool,
    pub cimd_allowed_origins: Vec<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let app_env = value("APP_ENV", "development");
        let app_url = url("APP_URL", "http://localhost:3000")?;
        let api_public_url = url("API_PUBLIC_URL", "http://localhost:4000")?;
        let api_internal_url = url("API_INTERNAL_URL", api_public_url.as_str())?;
        let auth_issuer = url("AUTH_ISSUER", api_public_url.as_str())?;
        let docs_url = url("DOCS_URL", "http://localhost:3003")?;
        let mcp_resource = url("MCP_RESOURCE", "http://localhost:4001/mcp")?;
        for (name, candidate) in [
            ("APP_URL", &app_url),
            ("DOCS_URL", &docs_url),
            ("API_PUBLIC_URL", &api_public_url),
            ("API_INTERNAL_URL", &api_internal_url),
            ("AUTH_ISSUER", &auth_issuer),
        ] {
            ensure_origin_url(name, candidate)?;
        }
        ensure_resource_url("MCP_RESOURCE", &mcp_resource)?;
        if api_public_url.origin() != auth_issuer.origin() {
            bail!("API_PUBLIC_URL and AUTH_ISSUER must use the same origin");
        }
        if app_env == "production" {
            for (name, candidate) in [
                ("APP_URL", &app_url),
                ("DOCS_URL", &docs_url),
                ("API_PUBLIC_URL", &api_public_url),
                ("AUTH_ISSUER", &auth_issuer),
                ("MCP_RESOURCE", &mcp_resource),
            ] {
                if candidate.scheme() != "https" {
                    bail!("{name} must use HTTPS in production");
                }
            }
        }
        let oidc_json = value("OIDC_PROVIDERS_JSON", "{}");
        let oidc_providers = OidcProviders::from_json(&oidc_json)
            .context("OIDC_PROVIDERS_JSON must be an object keyed by provider slug")?;
        validate_oidc_providers(&oidc_providers, app_env == "production")?;
        let cimd_allowed_origins = parse_cimd_origins(&value(
            "CIMD_ALLOWED_ORIGINS",
            "https://chatgpt.com,https://claude.ai",
        ))?;
        Ok(Self {
            app_env,
            app_name: value("APP_NAME", "Agent SaaS Starter"),
            app_url,
            docs_url,
            api_public_url,
            api_internal_url,
            auth_issuer,
            mcp_resource,
            database_url: required("DATABASE_URL")?,
            bind: value("API_BIND", "127.0.0.1:4000")
                .parse()
                .context("API_BIND must be a socket address")?,
            private_key_path: value("AUTH_PRIVATE_KEY_PATH", ".local/auth-private.pem").into(),
            public_key_path: value("AUTH_PUBLIC_KEY_PATH", ".local/auth-public.pem").into(),
            key_id: value("AUTH_KEY_ID", "starter-dev-1"),
            session_ttl: seconds("SESSION_TTL_SECONDS", 30 * 24 * 60 * 60)?,
            access_token_ttl: seconds("ACCESS_TOKEN_TTL_SECONDS", 15 * 60)?,
            refresh_token_ttl: seconds("REFRESH_TOKEN_TTL_SECONDS", 30 * 24 * 60 * 60)?,
            auth_code_ttl: seconds("AUTH_CODE_TTL_SECONDS", 5 * 60)?,
            auth_request_ttl: seconds("AUTH_REQUEST_TTL_SECONDS", 10 * 60)?,
            handoff_ttl: seconds("HANDOFF_TTL_SECONDS", 2 * 60)?,
            oidc_providers,
            dcr_enabled: boolean("DCR_ENABLED", true)?,
            cimd_allowed_origins,
        })
    }

    pub fn production(&self) -> bool {
        self.app_env == "production"
    }
}

fn required(name: &str) -> anyhow::Result<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("{name} is required"))
}

fn value(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_string())
}

fn url(name: &str, default: &str) -> anyhow::Result<Url> {
    Url::parse(&value(name, default)).with_context(|| format!("{name} must be an absolute URL"))
}

fn ensure_origin_url(name: &str, url: &Url) -> anyhow::Result<()> {
    ensure_resource_url(name, url)?;
    if url.path() != "/" {
        bail!("{name} must be an origin URL without a path");
    }
    Ok(())
}

fn ensure_resource_url(name: &str, url: &Url) -> anyhow::Result<()> {
    if !matches!(url.scheme(), "http" | "https") {
        bail!("{name} must use HTTP or HTTPS");
    }
    if url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
    {
        bail!("{name} must not include user information, query, or fragment");
    }
    Ok(())
}

fn parse_cimd_origins(value: &str) -> anyhow::Result<Vec<String>> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            let url = Url::parse(value).context("CIMD_ALLOWED_ORIGINS contains an invalid URL")?;
            if url.scheme() != "https"
                || url.path() != "/"
                || url.query().is_some()
                || url.fragment().is_some()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                bail!("CIMD_ALLOWED_ORIGINS entries must be HTTPS origins");
            }
            Ok(url.origin().ascii_serialization())
        })
        .collect()
}

fn validate_oidc_providers(providers: &OidcProviders, production: bool) -> anyhow::Result<()> {
    for (slug, provider) in &providers.0 {
        if slug.is_empty()
            || slug.len() > 48
            || !slug
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        {
            bail!("OIDC provider slugs must use 1-48 ASCII letters, numbers, or hyphens");
        }
        if provider.client_id.trim().is_empty() || provider.client_secret.trim().is_empty() {
            bail!("OIDC provider {slug} must include a client_id and client_secret");
        }
        let issuer = Url::parse(&provider.issuer)
            .with_context(|| format!("OIDC provider {slug} has an invalid issuer"))?;
        ensure_resource_url("OIDC provider issuer", &issuer)?;
        if production && issuer.scheme() != "https" {
            bail!("OIDC provider {slug} must use HTTPS in production");
        }
        if !provider.scopes.iter().any(|scope| scope == "openid")
            || !provider.scopes.iter().any(|scope| scope == "email")
        {
            bail!("OIDC provider {slug} must request the openid and email scopes");
        }
    }
    Ok(())
}

fn seconds(name: &str, default: u64) -> anyhow::Result<Duration> {
    let raw = value(name, &default.to_string());
    let value = raw
        .parse::<u64>()
        .with_context(|| format!("{name} must be a positive integer"))?;
    if value == 0 {
        bail!("{name} must be positive");
    }
    Ok(Duration::from_secs(value))
}

fn boolean(name: &str, default: bool) -> anyhow::Result<bool> {
    match value(name, if default { "true" } else { "false" })
        .to_ascii_lowercase()
        .as_str()
    {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => bail!("{name} must be a boolean"),
    }
}
