//! Browser terminal routes: one-time ticket issuance and the
//! WebSocket PTY bridge.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{
        Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use openpanel_app::WebTerminalService;
use openpanel_domain::web_terminal::{CloseReason, TerminalError};
use serde::Deserialize;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Idle window before the server closes the session.
const IDLE_TIMEOUT_SECS: u64 = 300;

/// Build routes nested under `/api/v1`.
pub fn router(service: Arc<WebTerminalService>) -> Router {
    Router::new()
        .route("/terminal/ticket", post(issue_ticket))
        .route("/terminal/session", get(ws_session))
        .with_state(service)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TicketInput {
    site_id: Uuid,
}

async fn issue_ticket(
    State(service): State<Arc<WebTerminalService>>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<TicketInput>,
) -> ApiResult<Json<openpanel_domain::web_terminal::TerminalTicket>> {
    let ticket = service
        .issue_ticket(&user, input.site_id)
        .await
        .map_err(map)?;
    Ok(Json(ticket))
}

#[derive(Deserialize)]
struct SessionQuery {
    ticket: String,
}

/// Reject browser upgrades whose Origin does not match the panel host.
fn origin_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(axum::http::header::ORIGIN) else {
        // Non-browser clients carry no Origin header.
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let origin_authority = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
        .unwrap_or(origin);
    match headers
        .get(axum::http::header::HOST)
        .and_then(|host| host.to_str().ok())
    {
        Some(host) => origin_authority.eq_ignore_ascii_case(host),
        None => false,
    }
}

async fn ws_session(
    State(service): State<Arc<WebTerminalService>>,
    Query(query): Query<SessionQuery>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !origin_allowed(&headers) {
        return (StatusCode::FORBIDDEN, "origin mismatch").into_response();
    }
    upgrade.on_upgrade(move |socket| bridge(service, query.ticket, socket))
}

async fn bridge(service: Arc<WebTerminalService>, token: String, socket: WebSocket) {
    let (mut sink, mut client_input) = socket.split();

    // Consume the ticket and spawn the scoped PTY.
    let (site_user, cwd) = match service.peek_ticket_target(&token).await {
        Ok(target) => target,
        Err(_) => {
            let _ = sink.send(Message::Close(None)).await;
            return;
        }
    };
    let (session_id, stream) = match service.start_session(&token, &site_user, &cwd).await {
        Ok(result) => result,
        Err(error) => {
            let code = match error {
                TerminalError::Used | TerminalError::Expired | TerminalError::UnknownTicket => {
                    1008u16
                }
                TerminalError::SessionCap => 1013,
                _ => 1011,
            };
            let _ = sink
                .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code,
                    reason: error.to_string().into(),
                })))
                .await;
            return;
        }
    };

    // Reader thread: PTY output -> bounded channel -> websocket sink.
    // The reader tracks unread bytes against the configured output
    // budget; a client that stops reading while the PTY keeps producing
    // overflows the budget and the session is force-closed.
    let output_budget = service.output_buffer_bytes();
    let stream = std::sync::Arc::new(std::sync::Mutex::new(stream));
    let (output_tx, mut output_rx) = mpsc::channel::<Vec<u8>>(256);
    let outstanding = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let overflowed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reader_stream = Arc::clone(&stream);
    let reader_outstanding = Arc::clone(&outstanding);
    let reader_overflowed = Arc::clone(&overflowed);
    let reader_handle = tokio::task::spawn_blocking(move || {
        loop {
            let mut buf = [0u8; 4096];
            let read = {
                // A poisoned mutex means the PTY is already gone; treat
                // it as end-of-stream.
                let Ok(mut guard) = reader_stream.lock() else {
                    break;
                };
                guard.read(&mut buf)
            };
            match read {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    use std::sync::atomic::Ordering;
                    let unread = reader_outstanding.fetch_add(n, Ordering::SeqCst) + n;
                    if unread > output_budget {
                        reader_overflowed.store(true, Ordering::SeqCst);
                        break;
                    }
                    if output_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut idle = tokio::time::interval(std::time::Duration::from_secs(IDLE_TIMEOUT_SECS));
    idle.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // `interval` completes its first tick immediately; consume it so a
    // fresh session is not judged idle before any activity.
    idle.tick().await;
    let mut closed_reason = CloseReason::ClientClosed;

    loop {
        tokio::select! {
            maybe_output = output_rx.recv() => {
                match maybe_output {
                    Some(bytes) => {
                        use std::sync::atomic::Ordering;
                        outstanding.fetch_sub(bytes.len(), Ordering::SeqCst);
                        if sink
                            .send(Message::Binary(bytes.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    None => {
                        if overflowed.load(std::sync::atomic::Ordering::SeqCst) {
                            closed_reason = CloseReason::Overflow;
                        }
                        break;
                    }
                }
            }
            maybe_input = client_input.next() => {
                match maybe_input {
                    Some(Ok(Message::Binary(bytes))) => {
                        let stream = Arc::clone(&stream);
                        let write_result = tokio::task::spawn_blocking(move || {
                            let Ok(mut guard) = stream.lock() else {
                                return Err(std::io::Error::other("pty mutex poisoned"));
                            };
                            guard.write(&bytes)
                        })
                        .await;
                        if write_result.is_err()
                            || matches!(write_result, Ok(Err(_)))
                        {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            _ = idle.tick() => {
                closed_reason = CloseReason::IdleTimeout;
                break;
            }
        }
    }

    let _ = sink.send(Message::Close(None)).await;
    let _ = service.close_session(session_id, closed_reason).await;
    reader_handle.abort();
}

fn map(error: TerminalError) -> ApiError {
    match error {
        TerminalError::Forbidden => ApiError::Forbidden,
        TerminalError::SiteNotFound(message) => ApiError::NotFound(message),
        TerminalError::Suspended => ApiError::Forbidden,
        TerminalError::Disabled => ApiError::NotFound("terminal disabled".into()),
        TerminalError::Used
        | TerminalError::Expired
        | TerminalError::UnknownTicket
        | TerminalError::SessionCap => ApiError::Unprocessable(error.to_string()),
        TerminalError::Failure(message) => ApiError::Internal(message),
    }
}
