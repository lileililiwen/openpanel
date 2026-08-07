//! Databases HTTP routes.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use openpanel_app::DatabasesService;
use openpanel_domain::DatabaseError;
use uuid::Uuid;

use crate::dto::{CreateDatabaseRequest, CreatedDatabaseResponse, DatabaseDto};
use crate::error::{ApiError, ApiResult};
use crate::extract::AuthUser;

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
