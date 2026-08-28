use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rsa::{
    RsaPrivateKey, RsaPublicKey,
    pkcs8::{DecodePrivateKey, DecodePublicKey},
    traits::PublicKeyParts,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthContext {
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub role: String,
    pub scopes: Vec<String>,
    pub client_id: String,
    pub token_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub exp: u64,
    pub iat: u64,
    pub nbf: u64,
    pub jti: String,
    pub scope: String,
    pub client_id: String,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdTokenClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub exp: u64,
    pub iat: u64,
    pub auth_time: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Jwk {
    pub kty: &'static str,
    #[serde(rename = "use")]
    pub use_: &'static str,
    pub alg: &'static str,
    pub kid: String,
    pub n: String,
    pub e: String,
}

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("invalid signing key: {0}")]
    InvalidKey(String),
    #[error("token could not be signed: {0}")]
    Sign(String),
    #[error("token is invalid: {0}")]
    InvalidToken(String),
    #[error("token does not include every required scope")]
    MissingScope,
    #[error("token identifier is invalid")]
    InvalidTokenId,
    #[error("token subject is invalid")]
    InvalidSubject,
}

#[derive(Clone)]
pub struct JwtIssuer {
    issuer: String,
    kid: String,
    encoding: Arc<EncodingKey>,
    decoding: Arc<DecodingKey>,
    jwk: Jwk,
}

#[derive(Clone)]
pub struct JwtVerifier {
    issuer: String,
    decoding: Arc<DecodingKey>,
}

impl JwtIssuer {
    pub fn from_pem(
        issuer: impl Into<String>,
        kid: impl Into<String>,
        private_pem: &[u8],
        public_pem: &[u8],
    ) -> Result<Self, JwtError> {
        let issuer = issuer.into();
        let kid = kid.into();
        let private_text = std::str::from_utf8(private_pem)
            .map_err(|error| JwtError::InvalidKey(error.to_string()))?;
        let private = RsaPrivateKey::from_pkcs8_pem(private_text)
            .map_err(|error| JwtError::InvalidKey(error.to_string()))?;
        let public = RsaPublicKey::from(&private);
        let public_from_file = RsaPublicKey::from_public_key_pem(
            std::str::from_utf8(public_pem)
                .map_err(|error| JwtError::InvalidKey(error.to_string()))?,
        )
        .map_err(|error| JwtError::InvalidKey(error.to_string()))?;
        if public.n() != public_from_file.n() || public.e() != public_from_file.e() {
            return Err(JwtError::InvalidKey(
                "public and private keys do not match".to_string(),
            ));
        }
        let encoding = EncodingKey::from_rsa_pem(private_pem)
            .map_err(|error| JwtError::InvalidKey(error.to_string()))?;
        let decoding = DecodingKey::from_rsa_pem(public_pem)
            .map_err(|error| JwtError::InvalidKey(error.to_string()))?;
        let jwk = Jwk {
            kty: "RSA",
            use_: "sig",
            alg: "RS256",
            kid: kid.clone(),
            n: URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()),
            e: URL_SAFE_NO_PAD.encode(public.e().to_bytes_be()),
        };
        Ok(Self {
            issuer,
            kid,
            encoding: Arc::new(encoding),
            decoding: Arc::new(decoding),
            jwk,
        })
    }

    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    pub fn jwk(&self) -> &Jwk {
        &self.jwk
    }

    pub fn verifier(&self) -> JwtVerifier {
        JwtVerifier {
            issuer: self.issuer.clone(),
            decoding: self.decoding.clone(),
        }
    }

    pub fn sign_access_token(
        &self,
        context: &AuthContext,
        audience: &str,
        issued_at: u64,
        expires_at: u64,
    ) -> Result<String, JwtError> {
        let claims = AccessTokenClaims {
            iss: self.issuer.clone(),
            sub: context.user_id.to_string(),
            aud: audience.to_string(),
            exp: expires_at,
            iat: issued_at,
            nbf: issued_at.saturating_sub(5),
            jti: context.token_id.to_string(),
            scope: context.scopes.join(" "),
            client_id: context.client_id.clone(),
            workspace_id: context.workspace_id,
            project_id: context.project_id,
            role: context.role.clone(),
        };
        self.sign(&claims)
    }

    pub fn sign_id_token(&self, claims: &IdTokenClaims) -> Result<String, JwtError> {
        self.sign(claims)
    }

    fn sign<T: Serialize>(&self, claims: &T) -> Result<String, JwtError> {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.kid.clone());
        encode(&header, claims, &self.encoding).map_err(|error| JwtError::Sign(error.to_string()))
    }
}

impl JwtVerifier {
    pub fn from_public_pem(issuer: impl Into<String>, public_pem: &[u8]) -> Result<Self, JwtError> {
        Ok(Self {
            issuer: issuer.into(),
            decoding: Arc::new(
                DecodingKey::from_rsa_pem(public_pem)
                    .map_err(|error| JwtError::InvalidKey(error.to_string()))?,
            ),
        })
    }

    pub fn verify_access_token(
        &self,
        token: &str,
        audience: &str,
        required_scopes: &[&str],
    ) -> Result<AuthContext, JwtError> {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&[audience]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub", "nbf"]);
        validation.validate_nbf = true;
        validation.leeway = 30;
        let token = decode::<AccessTokenClaims>(token, &self.decoding, &validation)
            .map_err(|error| JwtError::InvalidToken(error.to_string()))?;
        let scopes = token
            .claims
            .scope
            .split_ascii_whitespace()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if !required_scopes
            .iter()
            .all(|required| scopes.iter().any(|scope| scope == required || scope == "*"))
        {
            return Err(JwtError::MissingScope);
        }
        let user_id = Uuid::parse_str(&token.claims.sub).map_err(|_| JwtError::InvalidSubject)?;
        let token_id = Uuid::parse_str(&token.claims.jti).map_err(|_| JwtError::InvalidTokenId)?;
        Ok(AuthContext {
            user_id,
            workspace_id: token.claims.workspace_id,
            project_id: token.claims.project_id,
            role: token.claims.role,
            scopes,
            client_id: token.claims.client_id,
            token_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core_06::OsRng;
    use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};

    #[test]
    fn access_tokens_are_issuer_audience_and_scope_bound() {
        let private = RsaPrivateKey::new(&mut OsRng, 2048).unwrap();
        let public = RsaPublicKey::from(&private);
        let private_pem = private.to_pkcs8_pem(LineEnding::LF).unwrap();
        let public_pem = public.to_public_key_pem(LineEnding::LF).unwrap();
        let issuer = JwtIssuer::from_pem(
            "https://auth.example",
            "test-key",
            private_pem.as_bytes(),
            public_pem.as_bytes(),
        )
        .unwrap();
        let context = AuthContext {
            user_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            role: "owner".to_string(),
            scopes: vec!["project:read".to_string()],
            client_id: "client".to_string(),
            token_id: Uuid::new_v4(),
        };
        let now = chrono::Utc::now().timestamp() as u64;
        let token = issuer
            .sign_access_token(&context, "https://mcp.example/mcp", now, now + 300)
            .unwrap();
        let verified = issuer
            .verifier()
            .verify_access_token(&token, "https://mcp.example/mcp", &["project:read"])
            .unwrap();
        assert_eq!(verified, context);
        assert!(
            issuer
                .verifier()
                .verify_access_token(&token, "https://other.example/mcp", &["project:read"])
                .is_err()
        );
        assert!(matches!(
            issuer.verifier().verify_access_token(
                &token,
                "https://mcp.example/mcp",
                &["project:write"]
            ),
            Err(JwtError::MissingScope)
        ));
    }
}
