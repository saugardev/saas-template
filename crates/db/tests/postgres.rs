use sqlx::{
    AssertSqlSafe, PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{env, str::FromStr};
use uuid::Uuid;

#[tokio::test]
async fn migrations_and_refresh_rotation_invariants() -> anyhow::Result<()> {
    let Ok(database_url) = env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL is not set; skipping PostgreSQL integration test");
        return Ok(());
    };

    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;
    let schema = format!("starter_test_{}", Uuid::new_v4().simple());
    // `schema` contains only our fixed prefix and a UUID encoded as lowercase hex.
    sqlx::query(AssertSqlSafe(format!("CREATE SCHEMA \"{schema}\"")))
        .execute(&admin)
        .await?;
    let options = PgConnectOptions::from_str(&database_url)?.options([("search_path", &schema)]);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await?;

    let result = run_integration_checks(&pool).await;
    pool.close().await;
    sqlx::query(AssertSqlSafe(format!("DROP SCHEMA \"{schema}\" CASCADE")))
        .execute(&admin)
        .await?;
    admin.close().await;
    result
}

async fn run_integration_checks(pool: &PgPool) -> anyhow::Result<()> {
    starter_db::migrate(pool).await?;
    let users_table = sqlx::query_scalar::<_, Option<String>>("SELECT to_regclass('users')::text")
        .fetch_one(pool)
        .await?;
    assert_eq!(users_table.as_deref(), Some("users"));

    for table in ["email_verification_tokens", "password_reset_tokens"] {
        let present = sqlx::query_scalar::<_, Option<String>>("SELECT to_regclass($1)::text")
            .bind(table)
            .fetch_one(pool)
            .await?;
        assert!(
            present.is_none(),
            "removed mail table still exists: {table}"
        );
    }

    let user_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let grant_id = Uuid::new_v4();
    let family_id = Uuid::new_v4();
    let first_token_id = Uuid::new_v4();
    let replacement_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (user_id,email,name) VALUES ($1,$2,'Test User')")
        .bind(user_id)
        .bind(format!("{}@example.test", Uuid::new_v4()))
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO workspaces (workspace_id,name,slug,created_by) VALUES ($1,'Test','test',$2)",
    )
    .bind(workspace_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    sqlx::query("INSERT INTO memberships (membership_id,workspace_id,user_id,role) VALUES ($1,$2,$3,'owner')")
        .bind(Uuid::new_v4())
        .bind(workspace_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO projects (project_id,workspace_id,name,slug,created_by) VALUES ($1,$2,'Default','default',$3)")
        .bind(project_id)
        .bind(workspace_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO oauth_clients (client_id,client_name,registration_type,redirect_uris,scopes) VALUES ('test-client','Test','dynamic',$1,$2)")
        .bind(vec!["http://localhost/callback"])
        .bind(vec!["project:read", "offline_access"])
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO oauth_grants (grant_id,client_id,user_id,workspace_id,project_id,scopes,resource,auth_time) VALUES ($1,'test-client',$2,$3,$4,$5,'https://mcp.example/mcp',now())")
        .bind(grant_id)
        .bind(user_id)
        .bind(workspace_id)
        .bind(project_id)
        .bind(vec!["project:read", "offline_access"])
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO oauth_refresh_tokens (refresh_token_id,token_hash,grant_id,family_id,expires_at,consumed_at) VALUES ($1,$2,$3,$4,now()+interval '1 day',now())")
        .bind(first_token_id)
        .bind(vec![1_u8; 32])
        .bind(grant_id)
        .bind(family_id)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO oauth_refresh_tokens (refresh_token_id,token_hash,grant_id,family_id,parent_id,expires_at) VALUES ($1,$2,$3,$4,$5,now()+interval '1 day')")
        .bind(replacement_id)
        .bind(vec![2_u8; 32])
        .bind(grant_id)
        .bind(family_id)
        .bind(first_token_id)
        .execute(pool)
        .await?;
    sqlx::query("UPDATE oauth_refresh_tokens SET replaced_by=$1 WHERE refresh_token_id=$2")
        .bind(replacement_id)
        .bind(first_token_id)
        .execute(pool)
        .await?;

    let first = sqlx::query(
        "SELECT consumed_at,replaced_by FROM oauth_refresh_tokens WHERE refresh_token_id=$1",
    )
    .bind(first_token_id)
    .fetch_one(pool)
    .await?;
    assert!(
        first
            .get::<Option<chrono::DateTime<chrono::Utc>>, _>("consumed_at")
            .is_some()
    );
    assert_eq!(
        first.get::<Option<Uuid>, _>("replaced_by"),
        Some(replacement_id)
    );

    let mut transaction = pool.begin().await?;
    sqlx::query("UPDATE oauth_refresh_tokens SET revoked_at=now() WHERE family_id=$1")
        .bind(family_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("UPDATE oauth_grants SET revoked_at=now() WHERE grant_id=$1")
        .bind(grant_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    let revoked_tokens = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM oauth_refresh_tokens WHERE family_id=$1 AND revoked_at IS NOT NULL",
    )
    .bind(family_id)
    .fetch_one(pool)
    .await?;
    let grant_revoked = sqlx::query_scalar::<_, bool>(
        "SELECT revoked_at IS NOT NULL FROM oauth_grants WHERE grant_id=$1",
    )
    .bind(grant_id)
    .fetch_one(pool)
    .await?;
    assert_eq!(revoked_tokens, 2);
    assert!(grant_revoked);
    Ok(())
}
