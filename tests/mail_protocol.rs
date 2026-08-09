//! Opt-in contract tests for a disposable Postfix/Dovecot installation.
//!
//! Run with `cargo test --test mail_protocol -- --ignored` after setting
//! `OPENPANEL_MAIL_TEST_HOST`, `OPENPANEL_MAIL_TEST_USER`, and
//! `OPENPANEL_MAIL_TEST_PASSWORD`. The disposable fixture must expose SMTP
//! on 25, authenticated TLS submission on 465, and TLS IMAP on 993.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{
    io::{Read, Write},
    net::TcpStream,
    process::{Command, Stdio},
};

#[test]
#[ignore = "requires a disposable Postfix/Dovecot fixture"]
fn authenticated_submission_and_imap_work_while_third_party_relay_is_denied() {
    let host = required("OPENPANEL_MAIL_TEST_HOST");
    let user = required("OPENPANEL_MAIL_TEST_USER");
    let password = required("OPENPANEL_MAIL_TEST_PASSWORD");
    let auth = base64(&format!("\0{user}\0{password}"));

    let submission = tls_dialog(
        &host,
        465,
        &format!(
            "EHLO protocol.test\r\nAUTH PLAIN {auth}\r\nMAIL FROM:<{user}>\r\nRCPT TO:<{user}>\r\nDATA\r\nSubject: protocol contract\r\n\r\ntest\r\n.\r\nQUIT\r\n"
        ),
    );
    assert!(submission.contains("235"), "SMTP authentication failed");
    assert!(submission.contains("250"), "authenticated delivery failed");

    let imap = tls_dialog(
        &host,
        993,
        &format!("a1 LOGIN {user} {password}\r\na2 STATUS INBOX (MESSAGES)\r\na3 LOGOUT\r\n"),
    );
    assert!(imap.contains("a1 OK"), "IMAP authentication failed");
    assert!(imap.contains("a2 OK"), "IMAP mailbox access failed");

    let mut smtp = TcpStream::connect(format!("{host}:25")).expect("connect SMTP relay test");
    smtp
        .write_all(
            b"EHLO outside.test\r\nMAIL FROM:<sender@outside.test>\r\nRCPT TO:<target@remote.test>\r\nQUIT\r\n",
        )
        .expect("write SMTP relay test");
    smtp.shutdown(std::net::Shutdown::Write)
        .expect("shutdown SMTP");
    let mut response = String::new();
    smtp.read_to_string(&mut response)
        .expect("read SMTP response");
    assert!(
        response.contains(" 5.7.1") || response.contains("550") || response.contains("554"),
        "unauthenticated third-party relay was not rejected"
    );
}

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

fn base64(value: &str) -> String {
    let mut child = Command::new("openssl")
        .args(["base64", "-A"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn openssl base64");
    child
        .stdin
        .take()
        .expect("openssl stdin")
        .write_all(value.as_bytes())
        .expect("write openssl base64 input");
    let output = child.wait_with_output().expect("wait openssl base64");
    assert!(output.status.success(), "openssl base64 failed");
    String::from_utf8(output.stdout).expect("base64 output is UTF-8")
}

fn tls_dialog(host: &str, port: u16, input: &str) -> String {
    let endpoint = format!("{host}:{port}");
    let mut child = Command::new("openssl")
        .args(["s_client", "-quiet", "-connect", &endpoint])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn openssl s_client");
    child
        .stdin
        .take()
        .expect("openssl stdin")
        .write_all(input.as_bytes())
        .expect("write protocol dialog");
    let output = child.wait_with_output().expect("wait protocol dialog");
    assert!(output.status.success(), "TLS protocol dialog failed");
    String::from_utf8_lossy(&output.stdout).into_owned()
}
