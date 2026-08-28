use crate::{
    error::{ApiError, ApiResult},
    state::{AppState, session_token, slugify},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    routing::{delete, get, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use starter_auth::{hash_token, random_token};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/workspaces",
            get(list_workspaces).post(create_workspace),
        )
        .route("/api/v1/projects", post(create_project))
        .route("/api/v1/project-context", get(project_context))
        .route("/api/v1/session/selection", post(select_context))
        .route(
            "/api/v1/projects/{project_id}/api-keys",
            get(list_api_keys).post(create_api_key),
        )
        .route("/api/v1/api-keys/{api_key_id}", delete(revoke_api_key))
}

#[derive(Debug, Serialize)]
struct ProjectContextView {
    principal_type: &'static str,
    user_id: Option<Uuid>,
    workspace_id: Uuid,
    workspace_name: String,
    project_id: Uuid,
    project_name: String,
    role: Option<String>,
    scopes: Vec<String>,
}

async fn project_context(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<ProjectContextView>> {
    if session_token(&headers).is_some() {
        let session = state.session_user(&headers).await?;
        let row = sqlx::query("SELECT w.name AS workspace_name,p.name AS project_name FROM workspaces w JOIN projects p ON p.workspace_id=w.workspace_id WHERE w.workspace_id=$1 AND p.project_id=$2")
            .bind(session.workspace_id)
            .bind(session.project_id)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::internal)?;
        return Ok(Json(ProjectContextView {
            principal_type: "session",
            user_id: Some(session.user_id),
            workspace_id: session.workspace_id,
            workspace_name: row.get("workspace_name"),
            project_id: session.project_id,
            project_name: row.get("project_name"),
            role: Some(session.role),
            scopes: vec!["project:read".to_string()],
        }));
    }

    let key = api_key_token(&headers)
        .ok_or_else(|| ApiError::unauthorized("A valid session or API key is required"))?;
    let row = sqlx::query("SELECT k.api_key_id,k.scopes,p.project_id,p.name AS project_name,w.workspace_id,w.name AS workspace_name FROM api_keys k JOIN projects p ON p.project_id=k.project_id JOIN workspaces w ON w.workspace_id=p.workspace_id WHERE k.key_hash=$1 AND k.revoked_at IS NULL AND 'project:read'=ANY(k.scopes)")
        .bind(hash_token(key))
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::unauthorized("API key is invalid or revoked"))?;
    let api_key_id: Uuid = row.get("api_key_id");
    let _ = sqlx::query("UPDATE api_keys SET last_used_at=now() WHERE api_key_id=$1")
        .bind(api_key_id)
        .execute(&state.db)
        .await;
    Ok(Json(ProjectContextView {
        principal_type: "api_key",
        user_id: None,
        workspace_id: row.get("workspace_id"),
        workspace_name: row.get("workspace_name"),
        project_id: row.get("project_id"),
        project_name: row.get("project_name"),
        role: None,
        scopes: row.get("scopes"),
    }))
}

fn api_key_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("apikey")
        && token.starts_with("sk_")
        && !token.is_empty()
        && token.len() <= 512)
        .then_some(token)
}

#[derive(Debug, Serialize)]
struct WorkspaceView {
    id: Uuid,
    name: String,
    slug: String,
    role: String,
    projects: Vec<ProjectView>,
}

#[derive(Debug, Serialize)]
struct ProjectView {
    id: Uuid,
    name: String,
    slug: String,
}

async fn list_workspaces(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<WorkspaceView>>> {
    let session = state.session_user(&headers).await?;
    let rows = sqlx::query(
        r#"
        SELECT w.workspace_id,w.name AS workspace_name,w.slug AS workspace_slug,m.role,
               p.project_id,p.name AS project_name,p.slug AS project_slug
        FROM memberships m
        JOIN workspaces w ON w.workspace_id=m.workspace_id
        LEFT JOIN projects p ON p.workspace_id=w.workspace_id
        WHERE m.user_id=$1
        ORDER BY w.created_at,p.created_at
        "#,
    )
    .bind(session.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::internal)?;
    let mut workspaces = Vec::<WorkspaceView>::new();
    for row in rows {
        let workspace_id: Uuid = row.get("workspace_id");
        let index = workspaces
            .iter()
            .position(|workspace| workspace.id == workspace_id)
            .unwrap_or_else(|| {
                workspaces.push(WorkspaceView {
                    id: workspace_id,
                    name: row.get("workspace_name"),
                    slug: row.get("workspace_slug"),
                    role: row.get("role"),
                    projects: Vec::new(),
                });
                workspaces.len() - 1
            });
        if let Some(project_id) = row.get::<Option<Uuid>, _>("project_id") {
            workspaces[index].projects.push(ProjectView {
                id: project_id,
                name: row.get("project_name"),
                slug: row.get("project_slug"),
            });
        }
    }
    Ok(Json(workspaces))
}

#[derive(Debug, Deserialize)]
struct NameRequest {
    name: String,
}

async fn create_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<NameRequest>,
) -> ApiResult<(StatusCode, Json<WorkspaceView>)> {
    let session = state.session_user(&headers).await?;
    let name = clean_name(&request.name)?;
    let workspace_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let id_text = workspace_id.to_string();
    let slug = format!("{}-{}", slugify(name), &id_text[..8]);
    let mut transaction = state.db.begin().await.map_err(ApiError::internal)?;
    sqlx::query("INSERT INTO workspaces (workspace_id,name,slug,created_by) VALUES ($1,$2,$3,$4)")
        .bind(workspace_id)
        .bind(name)
        .bind(&slug)
        .bind(session.user_id)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    sqlx::query("INSERT INTO memberships (membership_id,workspace_id,user_id,role) VALUES ($1,$2,$3,'owner')")
        .bind(Uuid::new_v4())
        .bind(workspace_id)
        .bind(session.user_id)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    sqlx::query("INSERT INTO projects (project_id,workspace_id,name,slug,created_by) VALUES ($1,$2,'Default','default',$3)")
        .bind(project_id)
        .bind(workspace_id)
        .bind(session.user_id)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    transaction.commit().await.map_err(ApiError::internal)?;
    Ok((
        StatusCode::CREATED,
        Json(WorkspaceView {
            id: workspace_id,
            name: name.to_string(),
            slug,
            role: "owner".to_string(),
            projects: vec![ProjectView {
                id: project_id,
                name: "Default".to_string(),
                slug: "default".to_string(),
            }],
        }),
    ))
}

#[derive(Debug, Deserialize)]
struct CreateProjectRequest {
    workspace_id: Uuid,
    name: String,
}

async fn create_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateProjectRequest>,
) -> ApiResult<(StatusCode, Json<ProjectView>)> {
    let session = state.session_user(&headers).await?;
    let name = clean_name(&request.name)?;
    require_admin(&state, session.user_id, request.workspace_id).await?;
    let project_id = Uuid::new_v4();
    let id_text = project_id.to_string();
    let slug = format!("{}-{}", slugify(name), &id_text[..8]);
    sqlx::query("INSERT INTO projects (project_id,workspace_id,name,slug,created_by) VALUES ($1,$2,$3,$4,$5)")
        .bind(project_id)
        .bind(request.workspace_id)
        .bind(name)
        .bind(&slug)
        .bind(session.user_id)
        .execute(&state.db)
        .await
        .map_err(ApiError::internal)?;
    Ok((
        StatusCode::CREATED,
        Json(ProjectView {
            id: project_id,
            name: name.to_string(),
            slug,
        }),
    ))
}

#[derive(Debug, Deserialize)]
struct SelectContextRequest {
    workspace_id: Uuid,
    project_id: Uuid,
}

async fn select_context(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SelectContextRequest>,
) -> ApiResult<StatusCode> {
    let session = state.session_user(&headers).await?;
    let allowed = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM memberships m JOIN projects p ON p.workspace_id=m.workspace_id WHERE m.user_id=$1 AND m.workspace_id=$2 AND p.project_id=$3)",
    )
    .bind(session.user_id)
    .bind(request.workspace_id)
    .bind(request.project_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::internal)?;
    if !allowed {
        return Err(ApiError::forbidden(
            "The selected project is not available to this session",
        ));
    }
    sqlx::query(
        "UPDATE sessions SET active_workspace_id=$1,active_project_id=$2 WHERE session_id=$3",
    )
    .bind(request.workspace_id)
    .bind(request.project_id)
    .bind(session.session_id)
    .execute(&state.db)
    .await
    .map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
struct ApiKeyView {
    id: Uuid,
    name: String,
    prefix: String,
    scopes: Vec<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn list_api_keys(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<ApiKeyView>>> {
    let session = state.session_user(&headers).await?;
    require_project(&state, session.user_id, project_id).await?;
    let rows = sqlx::query("SELECT api_key_id,name,key_prefix,scopes,created_at,last_used_at FROM api_keys WHERE project_id=$1 AND revoked_at IS NULL ORDER BY created_at DESC")
        .bind(project_id)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(
        rows.into_iter()
            .map(|row| ApiKeyView {
                id: row.get("api_key_id"),
                name: row.get("name"),
                prefix: row.get("key_prefix"),
                scopes: row.get("scopes"),
                created_at: row.get("created_at"),
                last_used_at: row.get("last_used_at"),
            })
            .collect(),
    ))
}

#[derive(Debug, Deserialize)]
struct CreateApiKeyRequest {
    name: String,
    #[serde(default)]
    scopes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CreatedApiKey {
    api_key: ApiKeyView,
    secret: String,
}

async fn create_api_key(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<CreateApiKeyRequest>,
) -> ApiResult<(StatusCode, Json<CreatedApiKey>)> {
    let session = state.session_user(&headers).await?;
    require_project_admin(&state, session.user_id, project_id).await?;
    let name = clean_name(&request.name)?;
    let scopes = if request.scopes.is_empty() {
        vec!["project:read".to_string()]
    } else if request.scopes.iter().all(|scope| scope == "project:read") {
        request.scopes
    } else {
        return Err(ApiError::bad_request(
            "invalid_scope",
            "The starter API key supports only project:read",
        ));
    };
    let id = Uuid::new_v4();
    let material = random_token(32);
    let secret = format!("sk_{}_{material}", &id.to_string()[..8]);
    let prefix = secret.chars().take(14).collect::<String>();
    let created_at = Utc::now();
    sqlx::query("INSERT INTO api_keys (api_key_id,project_id,created_by,name,key_prefix,key_hash,scopes,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(id)
        .bind(project_id)
        .bind(session.user_id)
        .bind(name)
        .bind(&prefix)
        .bind(hash_token(&secret))
        .bind(&scopes)
        .bind(created_at)
        .execute(&state.db)
        .await
        .map_err(ApiError::internal)?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedApiKey {
            api_key: ApiKeyView {
                id,
                name: name.to_string(),
                prefix,
                scopes,
                created_at,
                last_used_at: None,
            },
            secret,
        }),
    ))
}

async fn revoke_api_key(
    State(state): State<AppState>,
    Path(api_key_id): Path<Uuid>,
    headers: HeaderMap,
) -> ApiResult<StatusCode> {
    let session = state.session_user(&headers).await?;
    let project_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT project_id FROM api_keys WHERE api_key_id=$1 AND revoked_at IS NULL",
    )
    .bind(api_key_id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::internal)?
    .ok_or_else(|| ApiError::not_found("API key was not found"))?;
    require_project_admin(&state, session.user_id, project_id).await?;
    sqlx::query("UPDATE api_keys SET revoked_at=now() WHERE api_key_id=$1")
        .bind(api_key_id)
        .execute(&state.db)
        .await
        .map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn require_admin(state: &AppState, user_id: Uuid, workspace_id: Uuid) -> ApiResult<()> {
    let role = sqlx::query_scalar::<_, String>(
        "SELECT role FROM memberships WHERE user_id=$1 AND workspace_id=$2",
    )
    .bind(user_id)
    .bind(workspace_id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::internal)?
    .ok_or_else(|| ApiError::forbidden("Workspace membership is required"))?;
    if role != "owner" && role != "admin" {
        return Err(ApiError::forbidden(
            "Workspace administrator access is required",
        ));
    }
    Ok(())
}

async fn require_project(state: &AppState, user_id: Uuid, project_id: Uuid) -> ApiResult<()> {
    let allowed = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM projects p JOIN memberships m ON m.workspace_id=p.workspace_id WHERE p.project_id=$1 AND m.user_id=$2)",
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::internal)?;
    if !allowed {
        return Err(ApiError::forbidden("Project access is required"));
    }
    Ok(())
}

async fn require_project_admin(state: &AppState, user_id: Uuid, project_id: Uuid) -> ApiResult<()> {
    let workspace_id =
        sqlx::query_scalar::<_, Uuid>("SELECT workspace_id FROM projects WHERE project_id=$1")
            .bind(project_id)
            .fetch_optional(&state.db)
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::not_found("Project was not found"))?;
    require_admin(state, user_id, workspace_id).await
}

fn clean_name(value: &str) -> ApiResult<&str> {
    let value = value.trim();
    if !(2..=100).contains(&value.chars().count()) {
        return Err(ApiError::bad_request(
            "invalid_name",
            "Name must be between 2 and 100 characters",
        ));
    }
    Ok(value)
}
