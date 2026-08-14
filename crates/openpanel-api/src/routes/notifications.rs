//! Notification channel, subscription, test-send, and health REST surfaces.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use openpanel_app::notifications::{
    CreateSmtpChannel, CreateSubscription, CreateWebhookChannel, NotificationService,
    NotificationServiceError,
};
use openpanel_domain::notifications::{EventKind, TlsMode};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    extract::AuthUser,
};

/// Build routes nested at `/api/v1/notifications`.
pub fn router(service: Arc<NotificationService>) -> Router {
    Router::new()
        .route("/channels", get(list_channels).post(create_channel))
        .route("/channels/{id}", axum::routing::delete(disable_channel))
        .route("/channels/{id}/test", post(test_channel))
        .route(
            "/subscriptions",
            get(list_subscriptions).post(create_subscription),
        )
        .route(
            "/subscriptions/{id}",
            axum::routing::delete(disable_subscription),
        )
        .route("/health", get(health))
        .route("/deliveries/{id}", get(delivery))
        .with_state(service)
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ChannelRequest {
    Smtp {
        name: String,
        host: String,
        port: u16,
        username: String,
        password: String,
        from_addr: String,
        tls_mode: TlsMode,
        allowlist: Vec<String>,
    },
    Webhook {
        name: String,
        url: String,
        signing_secret: String,
        allowlist: Vec<String>,
    },
}

async fn create_channel(
    State(service): State<Arc<NotificationService>>,
    AuthUser(user, _): AuthUser,
    Json(request): Json<ChannelRequest>,
) -> ApiResult<(
    StatusCode,
    Json<openpanel_domain::notifications::ChannelMetadata>,
)> {
    let channel = match request {
        ChannelRequest::Smtp {
            name,
            host,
            port,
            username,
            password,
            from_addr,
            tls_mode,
            allowlist,
        } => {
            service
                .create_smtp(
                    &user,
                    CreateSmtpChannel {
                        name,
                        host,
                        port,
                        username,
                        password,
                        from_addr,
                        tls_mode,
                        allowlist,
                    },
                )
                .await
        }
        ChannelRequest::Webhook {
            name,
            url,
            signing_secret,
            allowlist,
        } => {
            service
                .create_webhook(
                    &user,
                    CreateWebhookChannel {
                        name,
                        url,
                        signing_secret,
                        allowlist,
                    },
                )
                .await
        }
    }
    .map_err(map_error)?;
    Ok((StatusCode::CREATED, Json(channel)))
}

async fn list_channels(
    State(service): State<Arc<NotificationService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<openpanel_domain::notifications::ChannelMetadata>>> {
    Ok(Json(service.channels(&user).await.map_err(map_error)?))
}

async fn disable_channel(
    State(service): State<Arc<NotificationService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    service
        .disable_channel(&user, id)
        .await
        .map_err(map_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct TestRequest {
    destination: String,
}

async fn test_channel(
    State(service): State<Arc<NotificationService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
    Json(request): Json<TestRequest>,
) -> ApiResult<StatusCode> {
    service
        .test_channel(&user, id, &request.destination)
        .await
        .map_err(map_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct SubscriptionRequest {
    channel_id: Uuid,
    destination: String,
    kind: EventKind,
    filter: serde_json::Value,
}

async fn create_subscription(
    State(service): State<Arc<NotificationService>>,
    AuthUser(user, _): AuthUser,
    Json(request): Json<SubscriptionRequest>,
) -> ApiResult<(
    StatusCode,
    Json<openpanel_app::notifications::SubscriptionView>,
)> {
    let value = service
        .create_subscription(
            &user,
            CreateSubscription {
                channel_id: request.channel_id,
                destination: request.destination,
                kind: request.kind,
                filter: request.filter,
            },
        )
        .await
        .map_err(map_error)?;
    Ok((StatusCode::CREATED, Json(value)))
}

async fn list_subscriptions(
    State(service): State<Arc<NotificationService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<openpanel_app::notifications::SubscriptionView>>> {
    Ok(Json(service.subscriptions(&user).await.map_err(map_error)?))
}

async fn disable_subscription(
    State(service): State<Arc<NotificationService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    service
        .disable_subscription(&user, id)
        .await
        .map_err(map_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn health(
    State(service): State<Arc<NotificationService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<openpanel_domain::notifications::ChannelHealth>>> {
    Ok(Json(service.health(&user).await.map_err(map_error)?))
}

async fn delivery(
    State(service): State<Arc<NotificationService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<openpanel_app::notifications::DeliveryView>> {
    Ok(Json(service.delivery(&user, id).await.map_err(map_error)?))
}

fn map_error(error: NotificationServiceError) -> ApiError {
    match error {
        NotificationServiceError::Forbidden => ApiError::Forbidden,
        NotificationServiceError::NotFound => ApiError::NotFound("notification entity".into()),
        NotificationServiceError::Invalid(error) => ApiError::BadRequest(error.to_string()),
        NotificationServiceError::DestinationRejected => ApiError::BadRequest(error.to_string()),
        NotificationServiceError::Internal(error) => ApiError::Internal(error),
    }
}
