use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
pub struct OidcProvider {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub label: Option<String>,
}

fn default_scopes() -> Vec<String> {
    vec!["openid".into(), "profile".into(), "email".into()]
}

#[derive(Debug, Clone, Default)]
pub struct OidcProviders(pub BTreeMap<String, OidcProvider>);

impl OidcProviders {
    pub fn from_json(value: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(value).map(Self)
    }

    pub fn get(&self, slug: &str) -> Option<&OidcProvider> {
        self.0.get(slug)
    }

    pub fn public_labels(&self) -> Vec<(String, String)> {
        self.0
            .iter()
            .map(|(slug, provider)| {
                (
                    slug.clone(),
                    provider.label.clone().unwrap_or_else(|| slug.clone()),
                )
            })
            .collect()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OidcDiscovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OidcTokenResponse {
    pub id_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcIdClaims {
    pub iss: String,
    pub sub: String,
    #[serde(default)]
    pub aud: serde_json::Value,
    #[serde(default)]
    pub azp: Option<String>,
    pub exp: u64,
    pub nonce: String,
    pub email: String,
    #[serde(default)]
    pub email_verified: bool,
    #[serde(default)]
    pub name: Option<String>,
}
