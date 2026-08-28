use crate::{config::Config, email::EmailSender, error::ApiError};
use axum::http::{HeaderMap, header};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::Row;
use starter_auth::{JwtIssuer, hash_token, random_token};
use starter_db::Database;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Database,
    pub jwt: JwtIssuer,
    pub http: reqwest::Client,
    pub email: EmailSender,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionUser {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    pub email_verified: bool,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub role: String,
}

impl AppState {
    pub async fn session_user(&self, headers: &HeaderMap) -> Result<SessionUser, ApiError> {
        let token = session_token(headers)
            .ok_or_else(|| ApiError::unauthorized("A valid application session is required"))?;
        let row = sqlx::query(
            r#"
            SELECT s.session_id, u.user_id, u.email, u.name, u.email_verified_at,
                   s.active_workspace_id, s.active_project_id, m.role, s.expires_at
            FROM sessions s
            JOIN users u ON u.user_id = s.user_id
            JOIN memberships m ON m.workspace_id = s.active_workspace_id AND m.user_id = s.user_id
            JOIN projects p ON p.project_id = s.active_project_id AND p.workspace_id = s.active_workspace_id
            WHERE s.token_hash = $1 AND s.revoked_at IS NULL AND s.expires_at > now()
            "#,
        )
        .bind(hash_token(token))
        .fetch_optional(&self.db)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::unauthorized("The application session is invalid or expired"))?;
        let session_id: Uuid = row.get("session_id");
        let _ = sqlx::query("UPDATE sessions SET last_seen_at = now() WHERE session_id = $1")
            .bind(session_id)
            .execute(&self.db)
            .await;
        Ok(SessionUser {
            session_id,
            user_id: row.get("user_id"),
            email: row.get("email"),
            name: row.get("name"),
            email_verified: row
                .get::<Option<DateTime<Utc>>, _>("email_verified_at")
                .is_some(),
            workspace_id: row.get("active_workspace_id"),
            project_id: row.get("active_project_id"),
            role: row.get("role"),
        })
    }

    pub async fn create_session(
        &self,
        user_id: Uuid,
        workspace_id: Uuid,
        project_id: Uuid,
    ) -> Result<String, ApiError> {
        let token = random_token(32);
        let expires_at = Utc::now()
            + chrono::Duration::from_std(self.config.session_ttl).map_err(ApiError::internal)?;
        sqlx::query(
            r#"
            INSERT INTO sessions (
                session_id, token_hash, user_id, active_workspace_id, active_project_id, expires_at
            ) VALUES ($1,$2,$3,$4,$5,$6)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(hash_token(&token))
        .bind(user_id)
        .bind(workspace_id)
        .bind(project_id)
        .bind(expires_at)
        .execute(&self.db)
        .await
        .map_err(ApiError::internal)?;
        Ok(token)
    }
}

pub fn session_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("session") && !token.trim().is_empty()).then_some(token.trim())
}

pub fn slugify(value: &str) -> String {
    let mut output = String::new();
    let mut separated = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            output.push(character);
            separated = false;
        } else if !separated && !output.is_empty() {
            output.push('-');
            separated = true;
        }
    }
    let output = output.trim_matches('-');
    if output.is_empty() {
        "workspace".to_string()
    } else {
        output.chars().take(48).collect()
    }
}
