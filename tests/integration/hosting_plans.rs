//! Hosting plans HTTP integration tests.

use serde_json::json;

use crate::common::*;

async fn owner_server() -> (TestServer, String) {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    (server, token)
}

#[tokio::test]
async fn hosting_plans_get_returns_empty_list() {
    let (server, token) = owner_server().await;
    let resp = server
        .client()
        .get(format!("{}/api/v1/hosting-plans", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .expect("list");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("body");
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn hosting_plans_create_then_list_returns_one_row() {
    let (server, token) = owner_server().await;
    let resp = server
        .client()
        .post(format!("{}/api/v1/hosting-plans", server.base_url()))
        .bearer_auth(&token)
        .json(&json!({
            "name": "Basic",
            "description": "starter",
            "quota_caps": {
                "disk_bytes": 1_073_741_824_u64,
                "bandwidth_bytes_per_month": 10_737_418_240_u64,
                "max_sites": 5,
                "max_databases": 5,
                "max_mail_domains": 1,
                "max_mailboxes": 5,
                "max_cron_jobs": 10,
                "max_api_tokens": 2,
                "max_fleet_agents": 0,
            }
        }))
        .send()
        .await
        .expect("create");
    assert_eq!(resp.status(), 200);
    let created: serde_json::Value = resp.json().await.expect("created");
    assert_eq!(created["name"], "Basic");

    let list = server
        .client()
        .get(format!("{}/api/v1/hosting-plans", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .expect("list");
    let body: Vec<serde_json::Value> = list.json().await.expect("list body");
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["name"], "Basic");
}

#[tokio::test]
async fn hosting_plans_duplicate_name_returns_409() {
    let (server, token) = owner_server().await;
    let caps = quota_caps();
    let _ = server
        .client()
        .post(format!("{}/api/v1/hosting-plans", server.base_url()))
        .bearer_auth(&token)
        .json(&json!({"name": "Basic", "description": "starter", "quota_caps": caps}))
        .send()
        .await
        .expect("first");
    let dup = server
        .client()
        .post(format!("{}/api/v1/hosting-plans", server.base_url()))
        .bearer_auth(&token)
        .json(&json!({"name": "Basic", "description": "dup", "quota_caps": caps}))
        .send()
        .await
        .expect("duplicate");
    assert_eq!(dup.status(), 409);
}

#[tokio::test]
async fn hosting_plans_assign_then_unassign_round_trips() {
    let (server, token) = owner_server().await;
    let caps = quota_caps();
    let created = server
        .client()
        .post(format!("{}/api/v1/hosting-plans", server.base_url()))
        .bearer_auth(&token)
        .json(&json!({"name": "Plus", "description": "advanced", "quota_caps": caps}))
        .send()
        .await
        .expect("create");
    let created: serde_json::Value = created.json().await.expect("body");
    let plan_id = created["id"].as_str().expect("plan_id").to_string();

    // Create a user to assign.
    let user = server
        .identity()
        .create_user_with_parents(
            "alice",
            "alice@example.com",
            "correct horse battery staple",
            openpanel_domain::Role::User,
            None,
            None,
            "owner",
        )
        .await
        .expect("create user");

    let assign = server
        .client()
        .post(format!(
            "{}/api/v1/hosting-plans/{}/assign",
            server.base_url(),
            plan_id
        ))
        .bearer_auth(&token)
        .json(&json!({"user_id": user.id()}))
        .send()
        .await
        .expect("assign");
    assert_eq!(assign.status(), 200);

    // Confirm the user now has the plan.
    let users = server
        .client()
        .get(format!("{}/api/v1/identity/users", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .expect("users");
    let users: Vec<serde_json::Value> = users.json().await.expect("users body");
    let alice = users
        .iter()
        .find(|u| u["username"] == "alice")
        .expect("alice");
    assert_eq!(alice["hosting_plan_id"].as_str().expect("plan_id"), plan_id);

    // Unassign.
    let unassign = server
        .client()
        .post(format!(
            "{}/api/v1/hosting-plans/{}/unassign",
            server.base_url(),
            plan_id
        ))
        .bearer_auth(&token)
        .json(&json!({"user_id": user.id()}))
        .send()
        .await
        .expect("unassign");
    assert_eq!(unassign.status(), 200);

    let users = server
        .client()
        .get(format!("{}/api/v1/identity/users", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .expect("users");
    let users: Vec<serde_json::Value> = users.json().await.expect("users body");
    let alice = users
        .iter()
        .find(|u| u["username"] == "alice")
        .expect("alice");
    assert!(alice["hosting_plan_id"].is_null());
}

#[tokio::test]
async fn hosting_plans_delete_in_use_refuses_with_409() {
    let (server, token) = owner_server().await;
    let caps = quota_caps();
    let created = server
        .client()
        .post(format!("{}/api/v1/hosting-plans", server.base_url()))
        .bearer_auth(&token)
        .json(&json!({"name": "Shared", "description": "shared", "quota_caps": caps}))
        .send()
        .await
        .expect("create");
    let created: serde_json::Value = created.json().await.expect("body");
    let plan_id = created["id"].as_str().expect("plan_id").to_string();

    let user = server
        .identity()
        .create_user_with_parents(
            "bob",
            "bob@example.com",
            "correct horse battery staple",
            openpanel_domain::Role::User,
            None,
            None,
            "owner",
        )
        .await
        .expect("create user");

    let _ = server
        .client()
        .post(format!(
            "{}/api/v1/hosting-plans/{}/assign",
            server.base_url(),
            plan_id
        ))
        .bearer_auth(&token)
        .json(&json!({"user_id": user.id()}))
        .send()
        .await
        .expect("assign");

    let del = server
        .client()
        .delete(format!(
            "{}/api/v1/hosting-plans/{}",
            server.base_url(),
            plan_id
        ))
        .bearer_auth(&token)
        .send()
        .await
        .expect("delete");
    assert_eq!(del.status(), 409);
}

fn quota_caps() -> serde_json::Value {
    json!({
        "disk_bytes": 1_073_741_824_u64,
        "bandwidth_bytes_per_month": 10_737_418_240_u64,
        "max_sites": 5,
        "max_databases": 5,
        "max_mail_domains": 1,
        "max_mailboxes": 5,
        "max_cron_jobs": 10,
        "max_api_tokens": 2,
        "max_fleet_agents": 0,
    })
}
