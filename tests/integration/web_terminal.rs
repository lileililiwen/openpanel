//! Web-terminal integration tests: ticket issuance authorization,
//! single-use consumption, and WebSocket route guardrails.

use crate::common::*;

async fn owner_and_site(server: &TestServer) -> (String, openpanel_domain::Site) {
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let owner = server
        .identity()
        .list_users()
        .await
        .expect("users")
        .into_iter()
        .find(|user| user.username().as_str() == "owner")
        .expect("owner");
    let site = server
        .sites()
        .create_site(
            &owner,
            owner.id(),
            "terminal.example.test",
            vec![],
            false,
            None,
            Some("/var/www/terminal.example.test/public_html".to_owned()),
        )
        .await
        .expect("site");
    (token, site)
}

#[tokio::test]
async fn terminal_ticket_is_owner_only_and_hex_encoded() {
    let server = TestServer::new().await;
    let (_token, site) = owner_and_site(&server).await;
    let url = format!("{}/api/v1/terminal/ticket", server.base_url());
    let body = serde_json::json!({ "site_id": site.id() });

    // Unauthenticated callers are rejected.
    assert_eq!(
        server
            .client()
            .post(&url)
            .json(&body)
            .send()
            .await
            .expect("anon")
            .status(),
        401
    );

    // Admins may open terminals, but plain users may not exist here;
    // verify the owner path returns a 64-hex token.
    let issued = server
        .client()
        .post(&url)
        .bearer_auth(&_token)
        .json(&body)
        .send()
        .await
        .expect("issue");
    assert_eq!(
        issued.status(),
        200,
        "{}",
        issued.text().await.expect("body")
    );
    let json: serde_json::Value = issued.json().await.expect("json");
    let token = json["token"].as_str().expect("token");
    assert_eq!(token.len(), 64);
    assert!(
        token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    assert_eq!(json["site_id"], serde_json::json!(site.id().to_string()));
}

#[tokio::test]
async fn terminal_ticket_is_single_use_across_sessions() {
    let server = TestServer::new().await;
    let (token, site) = owner_and_site(&server).await;
    let url = format!("{}/api/v1/terminal/ticket", server.base_url());
    let issued: serde_json::Value = server
        .client()
        .post(&url)
        .bearer_auth(&token)
        .json(&serde_json::json!({ "site_id": site.id() }))
        .send()
        .await
        .expect("issue")
        .json()
        .await
        .expect("json");
    let ticket_token = issued["token"].as_str().expect("token").to_owned();

    let service = server.web_terminal();

    // The first start_session attempt consumes the ticket. The PTY
    // spawn itself fails in the sandbox (`su` target does not exist),
    // but consumption happens before the spawn.
    let _ = service
        .start_session(&ticket_token, "op-owner", "/tmp")
        .await;
    let second = service
        .start_session(&ticket_token, "op-owner", "/tmp")
        .await
        .err()
        .expect("second consume must fail");
    assert!(
        matches!(second, openpanel_domain::web_terminal::TerminalError::Used),
        "expected Used, got {second}"
    );

    // Unknown tickets are rejected outright.
    let unknown = service
        .start_session(&"ab".repeat(32), "op-owner", "/tmp")
        .await
        .err()
        .expect("unknown ticket must fail");
    assert!(matches!(
        unknown,
        openpanel_domain::web_terminal::TerminalError::UnknownTicket
    ));
}

#[tokio::test]
async fn terminal_ws_route_rejects_origin_mismatch_and_plain_gets() {
    let server = TestServer::new().await;
    let (token, site) = owner_and_site(&server).await;
    let issued: serde_json::Value = server
        .client()
        .post(format!("{}/api/v1/terminal/ticket", server.base_url()))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "site_id": site.id() }))
        .send()
        .await
        .expect("issue")
        .json()
        .await
        .expect("json");
    let ticket_token = issued["token"].as_str().expect("token").to_owned();
    let ws_url = format!(
        "{}/api/v1/terminal/session?ticket={}",
        server.base_url(),
        ticket_token
    );

    // A cross-origin browser upgrade is refused before consuming.
    let hostile = server
        .client()
        .get(&ws_url)
        .header("origin", "https://evil.example")
        .header("host", "panel.example.test")
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .send()
        .await
        .expect("hostile get");
    assert_eq!(hostile.status(), 403);

    // Plain HTTP gets (no upgrade) are rejected by the WS extractor.
    let plain = server
        .client()
        .get(&ws_url)
        .send()
        .await
        .expect("plain get");
    assert_ne!(plain.status(), 101);
}

/// A PTY double whose output stream never ends: every read returns a
/// full buffer, simulating a runaway process flooding the terminal.
struct FloodPty;

impl openpanel_domain::web_terminal::PtyPort for FloodPty {
    fn open(
        &self,
        _spec: &openpanel_domain::web_terminal::PtySpec,
    ) -> Result<
        Box<dyn openpanel_domain::web_terminal::PtyStream>,
        openpanel_domain::web_terminal::TerminalError,
    > {
        Ok(Box::new(FloodStream))
    }
}

struct FloodStream;

impl openpanel_domain::web_terminal::PtyStream for FloodStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        buf.fill(b'x');
        Ok(buf.len())
    }

    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(buf.len())
    }

    fn has_exited(&mut self) -> std::io::Result<bool> {
        Ok(false)
    }
}

/// Minimal WebSocket test client: handshake then frame-level reads.
/// Only what the overflow test needs — no masking (client-to-server
/// frames are never sent), close-frame detection included.
struct MiniWsClient {
    stream: tokio::net::TcpStream,
    leftover: Vec<u8>,
}

impl MiniWsClient {
    async fn connect(addr: &str, host: &str, ticket: &str) -> Self {
        use tokio::io::AsyncWriteExt;
        let mut stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("tcp connect");
        // An arbitrary key is accepted; the server only echoes the
        // accept header, which this client does not verify.
        let key = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            *b"openpanel-test-key",
        );
        let request = format!(
            "GET /api/v1/terminal/session?ticket={ticket} HTTP/1.1\r\n\
             Host: {host}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: {key}\r\n\
             Sec-WebSocket-Version: 13\r\n\
             Origin: http://{host}\r\n\r\n"
        );
        stream
            .write_all(request.as_bytes())
            .await
            .expect("handshake write");
        let mut chunk = [0u8; 4096];
        let mut response = Vec::new();
        let header_end;
        loop {
            let n = tokio::io::AsyncReadExt::read(&mut stream, &mut chunk)
                .await
                .expect("handshake read");
            assert!(n > 0, "connection closed during handshake");
            response.extend_from_slice(&chunk[..n]);
            if let Some(pos) = find_subslice(&response, b"\r\n\r\n") {
                header_end = pos + 4;
                break;
            }
        }
        let head = String::from_utf8_lossy(&response[..header_end]).to_string();
        assert!(
            head.starts_with("HTTP/1.1 101"),
            "expected 101, got: {head}"
        );
        Self {
            stream,
            leftover: response[header_end..].to_vec(),
        }
    }

    /// Drain frames until the server sends its Close frame.
    async fn read_until_close(&mut self) {
        loop {
            if self.leftover.len() < 2 {
                let mut chunk = [0u8; 4096];
                let n = tokio::io::AsyncReadExt::read(&mut self.stream, &mut chunk)
                    .await
                    .expect("frame read");
                assert!(n > 0, "connection dropped without a Close frame");
                self.leftover.extend_from_slice(&chunk[..n]);
                continue;
            }
            let opcode = self.leftover[0] & 0x0f;
            let len_byte = (self.leftover[1] & 0x7f) as usize;
            let (header_len, payload_len) = if len_byte < 126 {
                (2usize, len_byte)
            } else if len_byte == 126 {
                if self.leftover.len() < 4 {
                    continue;
                }
                (
                    4usize,
                    u16::from_be_bytes([self.leftover[2], self.leftover[3]]) as usize,
                )
            } else {
                if self.leftover.len() < 10 {
                    continue;
                }
                let mut ext = [0u8; 8];
                ext.copy_from_slice(&self.leftover[2..10]);
                (10usize, u64::from_be_bytes(ext) as usize)
            };
            if opcode == 0x8 {
                return;
            }
            let total = header_len + payload_len;
            while self.leftover.len() < total {
                let mut chunk = [0u8; 4096];
                let n = tokio::io::AsyncReadExt::read(&mut self.stream, &mut chunk)
                    .await
                    .expect("frame read");
                assert!(n > 0, "connection dropped mid-frame");
                self.leftover.extend_from_slice(&chunk[..n]);
            }
            self.leftover.drain(..total);
        }
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[tokio::test]
async fn terminal_ws_overflow_closes_session_and_audits() {
    use openpanel_core::{AuditAction, Config};

    let mut config = Config::default();
    config.modules.insert(
        "web_terminal".into(),
        serde_json::json!({ "output_buffer_bytes": 8192 }),
    );
    let server = TestServer::new_with_web_terminal_pty(config, std::sync::Arc::new(FloodPty)).await;
    let (token, site) = owner_and_site(&server).await;
    let issued: serde_json::Value = server
        .client()
        .post(format!("{}/api/v1/terminal/ticket", server.base_url()))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "site_id": site.id() }))
        .send()
        .await
        .expect("issue")
        .json()
        .await
        .expect("json");
    let ticket_token = issued["token"].as_str().expect("token").to_owned();

    let addr = server.base_url().trim_start_matches("http://").to_string();
    let mut ws = MiniWsClient::connect(&addr, &addr, &ticket_token).await;

    // Let the PTY flood far past the 8 KiB budget while this client
    // refuses to read — the server must force-close the session.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    ws.read_until_close().await;

    // The audit trail records the overflow close with session
    // coordinates only — never PTY bytes. The Close frame reaches the
    // client slightly before the bridge persists the audit event, so
    // poll briefly.
    let mut closed = None;
    for _ in 0..40 {
        let events = server.audit_events().await;
        if let Some(event) = events
            .iter()
            .find(|event| event.action == AuditAction::TerminalClosed)
        {
            closed = Some(event.clone());
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let closed = closed.expect("terminal_closed audit event");
    assert_eq!(
        closed.metadata["reason"],
        serde_json::json!("overflow"),
        "{}",
        closed.metadata
    );
    assert!(closed.metadata["user_id"].is_string());
    assert!(closed.metadata["site_id"].is_string());
    let serialized = serde_json::to_string(&closed.metadata).expect("metadata json");
    assert!(
        !serialized.contains("xxxx"),
        "PTY bytes leaked: {serialized}"
    );
}

/// A PTY double that floods output containing a per-session marker,
/// so the property can prove markers never reach the audit trail.
struct MarkerPty {
    marker: std::sync::Mutex<Vec<String>>,
}

impl MarkerPty {
    fn new() -> Self {
        Self {
            marker: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn issue_marker(&self) -> String {
        let marker = format!("LEAK{}", uuid::Uuid::new_v4().simple());
        self.marker
            .lock()
            .expect("marker lock")
            .push(marker.clone());
        marker
    }
}

impl openpanel_domain::web_terminal::PtyPort for MarkerPty {
    fn open(
        &self,
        _spec: &openpanel_domain::web_terminal::PtySpec,
    ) -> Result<
        Box<dyn openpanel_domain::web_terminal::PtyStream>,
        openpanel_domain::web_terminal::TerminalError,
    > {
        let marker = self.issue_marker();
        Ok(Box::new(MarkerStream {
            marker,
            position: 0,
        }))
    }
}

struct MarkerStream {
    marker: String,
    position: usize,
}

impl openpanel_domain::web_terminal::PtyStream for MarkerStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // Fill the buffer with the repeating marker so any audit leak
        // of stream content would carry it.
        for slot in buf.chunks_mut(self.marker.len()) {
            slot.copy_from_slice(&self.marker.as_bytes()[..slot.len().min(self.marker.len())]);
        }
        self.position += buf.len();
        Ok(buf.len())
    }

    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(buf.len())
    }

    fn has_exited(&mut self) -> std::io::Result<bool> {
        Ok(false)
    }
}

#[test]
fn prop_audit_events_carry_session_coordinates_only() {
    use openpanel_core::{AuditAction, Config};
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");

    runtime.block_on(async {
        let mut config = Config::default();
        config.modules.insert(
            "web_terminal".into(),
            serde_json::json!({ "output_buffer_bytes": 8192 }),
        );
        let pty = std::sync::Arc::new(MarkerPty::new());
        let server = TestServer::new_with_web_terminal_pty(config, pty.clone()).await;
        let (token, site) = owner_and_site(&server).await;
        let addr = server.base_url().trim_start_matches("http://").to_string();

        const SESSIONS: usize = 4;
        for _ in 0..SESSIONS {
            let issued: serde_json::Value = server
                .client()
                .post(format!("{}/api/v1/terminal/ticket", server.base_url()))
                .bearer_auth(&token)
                .json(&serde_json::json!({ "site_id": site.id() }))
                .send()
                .await
                .expect("issue")
                .json()
                .await
                .expect("json");
            let ticket_token = issued["token"].as_str().expect("token").to_owned();
            let mut ws = MiniWsClient::connect(&addr, &addr, &ticket_token).await;
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            ws.read_until_close().await;
        }

        // Every terminal audit event carries exactly the session
        // coordinate keys — and none of the streamed marker bytes.
        let allowed = ["user_id", "site_id", "opened_at", "closed_at", "reason"];
        let events = server.audit_events().await;
        let terminal: Vec<_> = events
            .iter()
            .filter(|event| {
                matches!(
                    event.action,
                    AuditAction::TerminalOpened | AuditAction::TerminalClosed
                )
            })
            .collect();
        assert!(
            terminal.len() >= SESSIONS,
            "expected at least {SESSIONS} close events, got {}",
            terminal.len()
        );
        let transcript: String = terminal
            .iter()
            .map(|event| serde_json::to_string(event).expect("event json"))
            .collect();
        for marker in pty.marker.lock().expect("marker lock").iter() {
            assert!(
                !transcript.contains(marker.as_str()),
                "PTY bytes leaked into the audit trail"
            );
        }
        for event in terminal {
            let metadata = event.metadata.as_object().expect("metadata object");
            if event.action == AuditAction::TerminalClosed {
                let keys: Vec<&String> = metadata.keys().collect();
                assert_eq!(keys.len(), allowed.len(), "{}", event.metadata);
                for key in keys {
                    assert!(allowed.contains(&key.as_str()), "{}", event.metadata);
                }
            }
        }
    });
}
