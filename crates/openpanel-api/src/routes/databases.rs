//! Databases HTTP routes.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use openpanel_app::DatabasesService;
use openpanel_domain::DatabaseError;
use uuid::Uuid;

use crate::{
    dto::{CreateDatabaseRequest, CreatedDatabaseResponse, DatabaseDto},
    error::{ApiError, ApiResult},
    extract::AuthUser,
};

/// Builds the Axum sub-router for `/databases` routes.
pub fn router(svc: Arc<DatabasesService>) -> Router {
    Router::new()
        .route("/", get(list_databases).post(create_database))
        .route("/{id}", get(get_database).delete(delete_database))
        .route("/{id}/password", post(change_password))
        .with_state(svc)
}

async fn list_databases(
    State(svc): State<Arc<DatabasesService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<DatabaseDto>>> {
    let dbs = svc.list_databases(&user).await.map_err(map_db_err)?;
    Ok(Json(dbs.iter().map(DatabaseDto::from_database).collect()))
}

async fn create_database(
    State(svc): State<Arc<DatabasesService>>,
    AuthUser(user, _): AuthUser,
    Json(req): Json<CreateDatabaseRequest>,
) -> ApiResult<Json<CreatedDatabaseResponse>> {
    let owner_id = req.owner_id.unwrap_or(user.id());
    let (db, password) = svc
        .create_database(
            &user,
            owner_id,
            &req.owner_username,
            &req.suffix,
            req.charset,
        )
        .await
        .map_err(map_db_err)?;
    Ok(Json(CreatedDatabaseResponse {
        database: DatabaseDto::from_database(&db),
        password,
    }))
}

async fn get_database(
    State(svc): State<Arc<DatabasesService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<DatabaseDto>> {
    let db = svc.get_database(&user, id).await.map_err(map_db_err)?;
    Ok(Json(DatabaseDto::from_database(&db)))
}

async fn delete_database(
    State(svc): State<Arc<DatabasesService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    svc.delete_database(&user, id).await.map_err(map_db_err)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn change_password(
    State(svc): State<Arc<DatabasesService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let password = svc.change_password(&user, id).await.map_err(map_db_err)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "password": password,
    })))
}

fn map_db_err(e: DatabaseError) -> ApiError {
    match e {
        DatabaseError::Forbidden => ApiError::Forbidden,
        DatabaseError::NotFound(_) => ApiError::NotFound(e.to_string()),
        DatabaseError::InvalidName(_) | DatabaseError::InvalidCharset(_) => {
            ApiError::BadRequest(e.to_string())
        }
        DatabaseError::DuplicateDatabase(_) => ApiError::Conflict(e.to_string()),
        DatabaseError::MysqlMissing => {
            ApiError::Internal("mysql CLI not installed; install mysql-server".to_string())
        }
        DatabaseError::MasterKeyMissing => {
            ApiError::Internal("master key missing from config".to_string())
        }
        DatabaseError::MysqlError(_)
        | DatabaseError::Encryption(_)
        | DatabaseError::Decryption(_)
        | DatabaseError::Persistence(_)
        | DatabaseError::Io(_) => ApiError::Internal(e.to_string()),
    }
}

/// Builds the Axum sub-router for per-database remote access
/// (own state, nested beside the other `/databases` routers).
pub fn remote_access_router(ctx: Arc<openpanel_app::DbRemoteAccessContext>) -> Router {
    Router::new()
        .route(
            "/{id}/remote-access",
            axum::routing::put(put_remote_access).get(get_remote_access),
        )
        .with_state(ctx)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoteAccessInput {
    /// MySQL account user part.
    user: String,
    /// Database name.
    database: String,
    enabled: bool,
    allow_cidrs: Vec<String>,
    wildcard_opt_in: bool,
}

async fn get_remote_access(
    State(ctx): State<Arc<openpanel_app::DbRemoteAccessContext>>,
    AuthUser(caller, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let access = ctx
        .controller
        .get(&caller, id)
        .await
        .map_err(map_db_privilege)?;
    // Never echo password material — the ACL carries none.
    Ok(Json(serde_json::json!({
        "database_id": access.database_id,
        "enabled": access.enabled,
        "allow_cidrs": access.allow_cidrs,
        "wildcard_opt_in": access.wildcard_opt_in,
    })))
}

async fn put_remote_access(
    State(ctx): State<Arc<openpanel_app::DbRemoteAccessContext>>,
    AuthUser(caller, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<RemoteAccessInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let access = ctx
        .controller
        .apply(
            &caller,
            id,
            &input.user,
            &input.database,
            input.enabled,
            input.allow_cidrs,
            input.wildcard_opt_in,
            ctx.port.as_ref(),
        )
        .await
        .map_err(map_db_privilege)?;
    Ok(Json(serde_json::json!({
        "database_id": access.database_id,
        "enabled": access.enabled,
        "allow_cidrs": access.allow_cidrs,
        "wildcard_opt_in": access.wildcard_opt_in,
    })))
}

fn map_db_privilege(error: openpanel_domain::db_privileges::DbPrivilegeError) -> ApiError {
    use openpanel_domain::db_privileges::DbPrivilegeError;
    match error {
        DbPrivilegeError::Forbidden => ApiError::Forbidden,
        DbPrivilegeError::NotFound(_) => ApiError::NotFound(error.to_string()),
        DbPrivilegeError::EmptyAcl | DbPrivilegeError::WildcardAcl => ApiError::GlobalAccessLocked,
        DbPrivilegeError::PrefixTooBroad | DbPrivilegeError::InvalidPolicy(_) => {
            ApiError::Unprocessable(error.to_string())
        }
        DbPrivilegeError::GrantFailed { step } => {
            ApiError::Internal(format!("grant failed at step {step}"))
        }
        DbPrivilegeError::InvalidSsoToken
        | DbPrivilegeError::OutsideDatabase(_)
        | DbPrivilegeError::Persistence(_) => ApiError::Internal(error.to_string()),
    }
}
