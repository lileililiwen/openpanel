use crate::common::*;

fn csrf(html: &str) -> String {
    let marker = "name=\"_csrf\" value=\"";
    let start = html.find(marker).expect("csrf field") + marker.len();
    let end = html[start..].find('"').expect("csrf close");
    html[start..start + end].to_owned()
}

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
            "waf-integration.example.test",
            vec![],
            false,
            None,
            None,
        )
        .await
        .expect("site");
    (token, site)
}

fn put_body(rule_id: uuid::Uuid) -> serde_json::Value {
    serde_json::json!({
        "version": 1,
        "default_action": "allow",
        "rules": [{
            "kind": "path_block",
            "id": rule_id,
            "enabled": true,
            "priority": 10,
            "pattern": "/private",
            "method": null,
            "action": "deny"
        }]
    })
}

#[tokio::test]
async fn waf_rest_put_get_dry_run_and_strict_schema() {
    let server = TestServer::new().await;
    let (token, site) = owner_and_site(&server).await;
    let url = format!("{}/api/v1/sites/{}/waf", server.base_url(), site.id());
    let initial = server
        .client()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .expect("get initial");
    assert_eq!(initial.status(), 200);
    assert_eq!(
        initial.json::<serde_json::Value>().await.expect("json")["rules"],
        serde_json::json!([])
    );

    let rule_id = uuid::Uuid::new_v4();
    let saved = server
        .client()
        .put(&url)
        .bearer_auth(&token)
        .json(&put_body(rule_id))
        .send()
        .await
        .expect("put rules");
    assert_eq!(saved.status(), 200, "{}", saved.text().await.expect("body"));

    let config = std::fs::read_to_string(
        std::path::Path::new(&server.sandbox_path("active"))
            .join("waf-integration.example.test.conf"),
    )
    .expect("rendered site config");
    assert!(config.contains("# openpanel-waf"));
    assert!(
        config.find("# openpanel-waf").expect("waf") < config.find("location /").expect("location")
    );

    let dry = server
        .client()
        .post(format!("{url}/test"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "rule": put_body(rule_id)["rules"][0],
            "request": {"path":"/private/key","method":"GET","user_agent":"Browser","country":"US"}
        }))
        .send()
        .await
        .expect("dry run");
    assert_eq!(dry.status(), 200);
    let dry: serde_json::Value = dry.json().await.expect("dry json");
    assert_eq!(dry["would_match"], true);
    assert!(
        dry["snippet"]
            .as_str()
            .expect("snippet")
            .contains("/private")
    );

    for _ in 0..2 {
        server
            .waf()
            .sample_hit(openpanel_domain::waf::WafHit {
                site_id: site.id(),
                rule_id,
                kind: "path_block".to_owned(),
                action: openpanel_domain::waf::RuleAction::Deny,
                count: 1,
                last_triggered_at: chrono::Utc::now(),
            })
            .await
            .expect("sample hit");
    }
    let hits: serde_json::Value = server
        .client()
        .get(format!("{url}/hits"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("get hits")
        .json()
        .await
        .expect("hits json");
    assert_eq!(hits[0]["count"], 2);
    assert_eq!(hits[0]["kind"], "path_block");

    let mut invalid = put_body(rule_id);
    invalid["rules"][0]["shell"] = serde_json::json!("ignored");
    assert_eq!(
        server
            .client()
            .put(&url)
            .bearer_auth(&token)
            .json(&invalid)
            .send()
            .await
            .expect("invalid")
            .status(),
        422
    );
}

#[tokio::test]
async fn waf_is_owner_only_and_web_mutation_requires_csrf() {
    let server = TestServer::new().await;
    let (_token, site) = owner_and_site(&server).await;
    server
        .identity()
        .create_user(
            "operator",
            "operator@example.test",
            "correct horse battery staple",
            openpanel_domain::Role::Admin,
            "test",
        )
        .await
        .expect("admin");
    let admin = server
        .login("operator", "correct horse battery staple")
        .await;
    let api = format!("{}/api/v1/sites/{}/waf", server.base_url(), site.id());
    assert_eq!(
        server
            .client()
            .get(&api)
            .bearer_auth(admin)
            .send()
            .await
            .expect("admin get")
            .status(),
        403
    );

    let login = server
        .client()
        .post(format!("{}/login", server.base_url()))
        .form(&[
            ("username_or_email", "owner"),
            ("password", "correct horse battery staple"),
        ])
        .send()
        .await
        .expect("web login");
    let cookie = login.headers()[reqwest::header::SET_COOKIE]
        .to_str()
        .expect("cookie")
        .split(';')
        .next()
        .expect("cookie pair")
        .to_owned();
    let page_url = format!("{}/sites/{}/waf", server.base_url(), site.id());
    let page = server
        .client()
        .get(&page_url)
        .header("cookie", &cookie)
        .send()
        .await
        .expect("page");
    assert_eq!(page.status(), 200);
    let body = page.text().await.expect("html");
    assert!(body.contains("Web application firewall"));
    assert!(body.contains("Dry-run rule"));
    let token = csrf(&body);

    let rules =
        serde_json::to_string(&put_body(uuid::Uuid::new_v4())["rules"]).expect("rules json");
    assert_eq!(
        server
            .client()
            .post(&page_url)
            .header("cookie", &cookie)
            .form(&[
                ("_csrf", "wrong"),
                ("version", "1"),
                ("default_action", "allow"),
                ("rules_json", rules.as_str())
            ])
            .send()
            .await
            .expect("bad csrf")
            .status(),
        403
    );
    let saved = server
        .client()
        .post(&page_url)
        .header("cookie", &cookie)
        .form(&[
            ("_csrf", token.as_str()),
            ("version", "1"),
            ("default_action", "allow"),
            ("rules_json", rules.as_str()),
        ])
        .send()
        .await
        .expect("save");
    assert_eq!(saved.status(), 200);
    assert!(
        saved
            .text()
            .await
            .expect("saved html")
            .contains("WAF policy saved")
    );

    let dry_rule = format!(
        "{{\"kind\":\"path_block\",\"id\":\"{}\",\"enabled\":true,\"priority\":10,\"pattern\":\"/private\",\"method\":null,\"action\":\"deny\"}}",
        uuid::Uuid::new_v4()
    );
    let tested = server
        .client()
        .post(format!("{page_url}/test"))
        .header("cookie", &cookie)
        .form(&[
            ("_csrf", token.as_str()),
            ("rule_json", dry_rule.as_str()),
            (
                "request_json",
                r#"{"path":"/private/key","method":"GET","user_agent":"Browser","country":null}"#,
            ),
        ])
        .send()
        .await
        .expect("web dry run");
    assert_eq!(tested.status(), 200);
    assert!(
        tested
            .text()
            .await
            .expect("dry-run html")
            .contains("would_match=true")
    );
}
