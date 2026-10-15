//! Staging mail-capture API and inbox HTML surface integration tests.

use openpanel_app::site_staging::CreateSlotRequest;
use openpanel_domain::MailCapturePort;
use openpanel_test_support::TestServer;
use uuid::Uuid;

async fn fixture() -> (TestServer, String, Uuid) {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("capowner", "correct horse battery staple")
        .await;
    let user = server
        .identity()
        .list_users()
        .await
        .unwrap()
        .into_iter()
        .find(|user| user.username().as_str() == "capowner")
        .unwrap();
    let site = server
        .sites()
        .create_site(
            &user,
            user.id(),
            "cap.example.test",
            vec![],
            false,
            None,
            None,
        )
        .await
        .unwrap();
    server
        .staging()
        .create_slot(
            &user,
            site.id(),
            CreateSlotRequest {
                subdomain: None,
                document_root: None,
                sync_policy: None,
                php_version: None,
                schedule: None,
            },
        )
        .await
        .unwrap();
    (server, token, site.id())
}

fn csrf(body: &str) -> String {
    let marker = "name=\"_csrf\" value=\"";
    let start = body.find(marker).unwrap() + marker.len();
    body[start..].split('"').next().unwrap().to_owned()
}

#[tokio::test]
async fn api_toggle_list_read_and_purge() {
    let (server, token, site_id) = fixture().await;
    let toggled = server
        .client()
        .post(format!(
            "{}/api/v1/sites/{site_id}/staging/mail-capture",
            server.base_url()
        ))
        .bearer_auth(&token)
        .json(&serde_json::json!({"enabled": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(toggled.status(), 200);
    assert_eq!(
        toggled.json::<serde_json::Value>().await.unwrap()["mail_capture"],
        true
    );
    let base = format!("{}/api/v1/sites/{site_id}/mail-capture", server.base_url());
    let empty = server
        .client()
        .get(&base)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(empty.status(), 200);
    assert_eq!(
        empty.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!([])
    );

    let captured = server
        .mail_capture()
        .capture_for_mailbox(
            "form@cap.example.test",
            "visitor@elsewhere.test",
            "Contact form",
            "<p>hello body</p>\n\nhello body",
        )
        .await
        .unwrap();
    assert!(captured, "enabled slot must capture");

    let listed = server
        .client()
        .get(&base)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    let rows: serde_json::Value = listed.json().await.unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    let message_id = rows[0]["id"].as_str().unwrap().to_owned();
    let list_text = rows.to_string();
    assert!(!list_text.contains("hello body"), "list view is body-free");

    let read = server
        .client()
        .get(format!("{base}/{message_id}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(read.status(), 200);
    let detail: serde_json::Value = read.json().await.unwrap();
    assert_eq!(detail["subject"], "Contact form");
    assert!(detail["body"].as_str().unwrap().contains("hello body"));

    let purged = server
        .client()
        .post(format!("{base}/purge"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(purged.status(), 200);
    assert_eq!(
        purged.json::<serde_json::Value>().await.unwrap()["removed"],
        1
    );
    let after = server
        .client()
        .get(&base)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(
        after.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!([])
    );
}

#[tokio::test]
async fn api_purge_of_unknown_site_is_not_found() {
    let (server, token, _site_id) = fixture().await;
    let ghost = Uuid::new_v4();
    let response = server
        .client()
        .post(format!(
            "{}/api/v1/sites/{ghost}/mail-capture/purge",
            server.base_url()
        ))
        .bearer_auth(&token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn web_inbox_renders_detail_and_purge_requires_csrf() {
    let (server, token, site_id) = fixture().await;
    server
        .staging()
        .set_mail_capture(
            &server
                .identity()
                .list_users()
                .await
                .unwrap()
                .into_iter()
                .find(|user| user.username().as_str() == "capowner")
                .unwrap(),
            site_id,
            true,
        )
        .await
        .unwrap();
    server
        .mail_capture()
        .capture_for_mailbox(
            "form@cap.example.test",
            "visitor@elsewhere.test",
            "Contact form",
            "stored plaintext body",
        )
        .await
        .unwrap();
    let cookie = format!("openpanel_session={token}");
    let page = server
        .client()
        .get(format!("{}/mail/capture/{site_id}", server.base_url()))
        .header("cookie", cookie.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    let body = page.text().await.unwrap();
    assert!(body.contains("Staging mail capture"));
    assert!(body.contains("Contact form"));
    let message_id = {
        let listed = server
            .client()
            .get(format!(
                "{}/api/v1/sites/{site_id}/mail-capture",
                server.base_url()
            ))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        listed[0]["id"].as_str().unwrap().to_owned()
    };
    let detail = server
        .client()
        .get(format!(
            "{}/mail/capture/{site_id}/{message_id}",
            server.base_url()
        ))
        .header("cookie", cookie.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(detail.status(), 200);
    assert!(
        detail
            .text()
            .await
            .unwrap()
            .contains("stored plaintext body")
    );

    let denied = server
        .client()
        .post(format!(
            "{}/mail/capture/{site_id}/purge",
            server.base_url()
        ))
        .header("cookie", cookie.clone())
        .form(&[("_csrf", "wrong")])
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 403);
    let token_csrf = csrf(&body);
    let purged = server
        .client()
        .post(format!(
            "{}/mail/capture/{site_id}/purge",
            server.base_url()
        ))
        .header("cookie", cookie.clone())
        .form(&[("_csrf", token_csrf.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(purged.status(), 200);
    assert!(
        purged
            .text()
            .await
            .unwrap()
            .contains("Purged 1 captured message")
    );
}

#[tokio::test]
async fn web_staging_page_shows_capture_toggle() {
    let (server, token, site_id) = fixture().await;
    let page = server
        .client()
        .get(format!("{}/sites/{site_id}/staging", server.base_url()))
        .header("cookie", format!("openpanel_session={token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    let body = page.text().await.unwrap();
    assert!(
        body.contains("Mail capture"),
        "toggle section renders: {body}"
    );
    assert!(body.contains("Enable capture"), "default is off: {body}");
    let token_csrf = csrf(&body);
    let flipped = server
        .client()
        .post(format!(
            "{}/sites/{site_id}/staging/mail-capture",
            server.base_url()
        ))
        .header("cookie", format!("openpanel_session={token}"))
        .form(&[("_csrf", token_csrf.as_str()), ("enabled", "true")])
        .send()
        .await
        .unwrap();
    assert_eq!(flipped.status(), 303);
    let after = server
        .client()
        .get(format!("{}/sites/{site_id}/staging", server.base_url()))
        .header("cookie", format!("openpanel_session={token}"))
        .send()
        .await
        .unwrap();
    let after_body = after.text().await.unwrap();
    assert!(
        after_body.contains("Disable capture"),
        "toggle persisted: {after_body}"
    );
}
