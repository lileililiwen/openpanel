//! Tests for the databases web pages (extracted from `mod.rs` to respect the 1000-line file-length ceiling).

use chrono::Utc;
use openpanel_domain::databases::database::Database;

use super::*;

fn db(name: &str) -> Database {
    Database::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "admin",
        name,
        "utf8mb4",
        "admin",
    )
    .expect("valid database")
}

#[test]
fn list_renders_one_row_per_database() {
    let dbs = [db("appdb"), db("logs")];
    let user = openpanel_domain::User::restore(
        Uuid::new_v4(),
        openpanel_domain::Username::new("admin").expect("u"),
        openpanel_domain::Email::new("admin@example.com").expect("e"),
        "h".into(),
        openpanel_domain::Role::Owner,
        None,
        None,
        Utc::now(),
        None,
        None,
    );
    let rows = collect_rows(&dbs, &user);
    let out = list_fragment(&rows, true, "tok").into_string();
    assert!(out.contains("appdb"), "appdb: {out}");
    assert!(out.contains("logs"), "logs: {out}");
    assert!(out.contains("admin"), "owner: {out}");
    assert!(out.contains("utf8mb4"), "charset: {out}");
    assert!(out.contains("Add database"), "create action: {out}");
}

#[test]
fn list_hides_actions_for_non_managers() {
    let dbs = [db("appdb")];
    let out = list_fragment(
        &dbs.iter()
            .map(|d| DbRow {
                id: d.id(),
                name: d.name().to_string(),
                owner: d.created_by().to_string(),
                charset: d.charset().to_string(),
                status: d.status().as_str().to_string(),
                quota: None,
                backup_count: None,
            })
            .collect::<Vec<_>>(),
        false,
        "tok",
    )
    .into_string();
    assert!(!out.contains("Add database"), "no create: {out}");
    assert!(!out.contains("Delete"), "no delete: {out}");
    assert!(!out.contains("Rotate"), "no rotate: {out}");
}

#[test]
fn list_empty_state_renders() {
    let out = list_fragment(&[], true, "tok").into_string();
    assert!(out.contains("No databases yet"), "empty: {out}");
}

#[test]
fn list_renders_delete_via_layer_confirm() {
    let dbs = [db("appdb")];
    let rows: Vec<DbRow> = dbs
        .iter()
        .map(|d| DbRow {
            id: d.id(),
            name: d.name().to_string(),
            owner: d.created_by().to_string(),
            charset: d.charset().to_string(),
            status: d.status().as_str().to_string(),
            quota: None,
            backup_count: None,
        })
        .collect();
    let out = list_fragment(&rows, true, "tok").into_string();
    assert!(
        out.contains(&format!(
            "hx-get=\"/layer/confirm?action=delete-database&amp;id={}\"",
            rows[0].id
        )),
        "delete routes through layer confirm: {out}"
    );
    assert!(
        out.contains("hx-get=\"/layer/confirm?action=reveal-database-password"),
        "reveal routes through layer confirm: {out}"
    );
    assert!(!out.contains("hx-confirm"), "no native confirm: {out}");
    assert!(!out.contains("hx-delete="), "no raw delete: {out}");
}

#[test]
fn create_form_renders_all_fields() {
    let out = create_form("tok", None, None).into_string();
    for needle in [
        "name=\"owner_id\"",
        "name=\"owner_username\"",
        "name=\"suffix\"",
        "name=\"charset\"",
        "Create database",
    ] {
        assert!(out.contains(needle), "missing {needle}: {out}");
    }
}

#[test]
fn create_form_renders_inline_error() {
    let out = create_form("tok", Some("invalid owner id"), None).into_string();
    assert!(out.contains("invalid owner id"), "error: {out}");
    assert!(out.contains("role=\"alert\""), "alert role: {out}");
}

#[test]
fn password_panel_renders_plaintext_once() {
    let now = Utc::now();
    let out = password_panel_response(
        Ok("s3cret-plaintext-1234".to_string()),
        "tok",
        "rotated",
        now,
    )
    .into_string();
    assert!(out.contains("id=\"password-panel\""), "panel: {out}");
    assert!(out.contains("s3cret-plaintext-1234"), "plaintext: {out}");
    assert!(out.contains("rotated at"), "rotated at: {out}");
}

#[test]
fn password_panel_renders_error_on_failure() {
    let now = Utc::now();
    let out = password_panel_response(
        Err(openpanel_domain::databases::error::DatabaseError::NotFound(
            "x".into(),
        )),
        "tok",
        "revealed",
        now,
    )
    .into_string();
    assert!(out.contains("not found"), "error rendered: {out}");
    assert!(
        !out.contains("id=\"password-panel\""),
        "no panel on error: {out}"
    );
}

#[test]
fn detail_section_renders_metadata() {
    let database = db("appdb");
    let out = detail_section(&database, "admin", "tok", true).into_string();
    for needle in [
        "appdb",
        "Owner:",
        "admin",
        "Engine:",
        "Charset:",
        "Status:",
        "Created at:",
    ] {
        assert!(out.contains(needle), "missing {needle}: {out}");
    }
}
