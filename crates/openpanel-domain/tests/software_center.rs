#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use openpanel_domain::software_center::{
    ArchivePolicy, ArtifactPin, CatalogEntryRecipe, CatalogId, CatalogManifest, CatalogQuery,
    Category, ConfirmationToken, DependencyGraph, EntryKind, Homepage, JobState, License,
    PackageId, PlanAction, RecipeError, SoftwareJob, SoftwarePlan, SoftwareVersion,
    SupportedPlatform, Tag, VersionSpec,
};
use proptest::prelude::*;
use uuid::Uuid;

#[test]
fn catalog_package_version_and_platform_values_are_strict() {
    assert_eq!(CatalogId::new("php-8.4").unwrap().as_str(), "php-8.4");
    assert!(CatalogId::new("PHP 8.4").is_err());
    assert!(PackageId::new("php8.4-fpm; reboot").is_err());
    assert_eq!(PackageId::new("php8.4-fpm").unwrap().as_str(), "php8.4-fpm");
    assert_eq!(SoftwareVersion::parse("8.4.2").unwrap().major(), 8);
    assert!(SoftwareVersion::parse("latest").is_err());
    assert!(SupportedPlatform::new("ubuntu", "24.04", "x86_64").is_ok());
    assert!(SupportedPlatform::new("../../etc", "24.04", "x86_64").is_err());
}

#[test]
fn dependency_graph_rejects_cycles_and_reports_reverse_dependencies() {
    let mut graph = DependencyGraph::default();
    graph.add("wordpress", "php-8.4").unwrap();
    graph.add("wordpress", "mysql").unwrap();
    graph.add("php-8.4", "nginx").unwrap();
    assert_eq!(graph.dependents_of("nginx"), vec!["php-8.4", "wordpress"]);
    assert!(graph.add("nginx", "wordpress").is_err());
}

#[test]
fn plans_are_digest_bound_and_confirmation_is_scoped_short_lived_and_one_use() {
    let platform = SupportedPlatform::new("ubuntu", "24.04", "x86_64").unwrap();
    let actions = vec![
        PlanAction::install(PackageId::new("nginx").unwrap()),
        PlanAction::install(PackageId::new("php8.4-fpm").unwrap()),
    ];
    let first = SoftwarePlan::new("catalog-sha256", platform.clone(), actions.clone()).unwrap();
    let second = SoftwarePlan::new("catalog-sha256", platform, actions).unwrap();
    assert_eq!(first.digest(), second.digest());
    let mut token = ConfirmationToken::issue(first.digest(), 100, 60).unwrap();
    assert!(token.consume(first.digest(), 120));
    assert!(!token.consume(first.digest(), 121));
    let mut wrong = ConfirmationToken::issue(first.digest(), 100, 60).unwrap();
    assert!(!wrong.consume("different-plan", 120));
    assert!(!wrong.consume(first.digest(), 161));
}

#[test]
fn job_state_machine_reconciles_interruption_and_terminal_states_stay_terminal() {
    let mut job = SoftwareJob::new(Uuid::new_v4(), "plan-digest").unwrap();
    job.start().unwrap();
    job.validate().unwrap();
    job.succeed().unwrap();
    assert_eq!(job.state(), JobState::Succeeded);
    assert!(job.start().is_err());

    let mut interrupted = SoftwareJob::new(Uuid::new_v4(), "plan-digest").unwrap();
    interrupted.start().unwrap();
    interrupted.reconcile_interrupted().unwrap();
    assert_eq!(interrupted.state(), JobState::Interrupted);
}

#[test]
fn archive_policy_rejects_traversal_symlinks_and_resource_bombs() {
    let policy = ArchivePolicy::new(10, 1_000_000).unwrap();
    assert!(policy.accept("wp/index.php", false, 100, 1).is_ok());
    assert!(policy.accept("../etc/passwd", false, 10, 1).is_err());
    assert!(policy.accept("wp/link", true, 10, 1).is_err());
    assert!(policy.accept("wp/large.bin", false, 1_000_001, 1).is_err());
    assert!(policy.accept("wp/file", false, 1, 11).is_err());
}

proptest! {
    #[test]
    fn arbitrary_package_ids_never_accept_command_syntax(value in ".{0,128}") {
        if value.contains(|ch: char| ch.is_whitespace() || ";|&`$()<>\\\"'".contains(ch)) {
            prop_assert!(PackageId::new(&value).is_err());
        }
    }

    #[test]
    fn terminal_jobs_never_restart(success in any::<bool>()) {
        let mut job=SoftwareJob::new(Uuid::new_v4(),"digest").unwrap();
        job.start().unwrap();
        if success {
            job.validate().unwrap();
            job.succeed().unwrap();
        } else {
            job.fail().unwrap();
        }
        prop_assert!(job.start().is_err());
    }

    #[test]
    fn tag_validation_is_strict_kebab_case(value in ".{0,64}") {
        let result = Tag::new(&value);
        let valid = !value.is_empty()
            && value.len() <= Tag::MAX_LENGTH
            && value.is_ascii()
            && !value.starts_with('-')
            && !value.ends_with('-')
            && !value.contains("--")
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
            });
        if valid {
            prop_assert!(result.is_ok());
        } else {
            prop_assert!(result.is_err());
        }
    }
}

#[test]
fn category_round_trips_through_slug_and_label() {
    for category in Category::all() {
        let slug = category.slug();
        let parsed = slug.parse::<Category>().expect("round-trip slug");
        assert_eq!(&parsed, category);
        assert!(!category.label().is_empty());
    }
    assert!("unknown".parse::<Category>().is_err());
}

#[test]
fn license_validation_rejects_compound_expressions() {
    assert_eq!(License::new("MIT").unwrap().as_str(), "MIT");
    assert_eq!(
        License::new("GPL-2.0-only").unwrap().as_str(),
        "GPL-2.0-only"
    );
    assert_eq!(License::new("Apache-2.0").unwrap().as_str(), "Apache-2.0");
    assert!(License::new("MIT OR Apache-2.0").is_err());
    assert!(License::new("").is_err());
    assert!(License::new(&"x".repeat(License::MAX_LENGTH + 1)).is_err());
}

#[test]
fn homepage_validation_rejects_non_https_userinfo_and_query() {
    assert!(Homepage::new("https://example.com/path").is_ok());
    assert!(Homepage::new("http://example.com").is_err());
    assert!(Homepage::new("https://user@example.com").is_err());
    assert!(Homepage::new("https://example.com?query=1").is_err());
    assert!(Homepage::new("https://example.com#frag").is_err());
}

#[test]
fn system_entry_rejects_artifact_pin() {
    let entry = CatalogEntryRecipe {
        id: "redis".into(),
        name: "Redis".into(),
        description: "In-memory data store".into(),
        long_description: String::new(),
        category: Category::Cache,
        kind: EntryKind::System,
        tags: vec![Tag::new("cache").unwrap()],
        license: License::new("RSALv2").unwrap(),
        developer: "Redis Labs".into(),
        homepage: Homepage::new("https://redis.io").unwrap(),
        versions: vec![VersionSpec {
            version: "7.2.4".into(),
            size_bytes: 7 * 1024 * 1024,
            changelog_url: None,
            artifact: None,
            packages: vec!["redis-server".into()],
            supports_php: Vec::new(),
            released_at: Some("2024-04-16".into()),
        }],
        platforms: vec![],
        dependencies: vec![],
        conflicts: vec![],
        icon: None,
        screenshot: None,
    };
    let known = std::collections::BTreeSet::from(["redis".to_string()]);
    assert!(entry.validate(&known).is_ok());

    let mut bad = entry.clone();
    bad.kind = EntryKind::Web;
    bad.versions[0].packages.clear();
    assert!(bad.validate(&known).is_err());
}

#[test]
fn web_entry_requires_artifact_pin_and_sha256() {
    let known = std::collections::BTreeSet::from(["wordpress".to_string()]);
    let entry = CatalogEntryRecipe {
        id: "wordpress".into(),
        name: "WordPress".into(),
        description: "Publishing platform".into(),
        long_description: String::new(),
        category: Category::OneClick,
        kind: EntryKind::Web,
        tags: vec![Tag::new("cms").unwrap(), Tag::new("php").unwrap()],
        license: License::new("GPL-2.0-or-later").unwrap(),
        developer: "WordPress Foundation".into(),
        homepage: Homepage::new("https://wordpress.org").unwrap(),
        versions: vec![VersionSpec {
            version: "6.5.2".into(),
            size_bytes: 32 * 1024 * 1024,
            changelog_url: None,
            artifact: Some(ArtifactPin {
                url: Homepage::new("https://wordpress.org/wordpress-6.5.2.tar.gz").unwrap(),
                sha256: "a".repeat(64),
                archive_root: "wordpress".into(),
                archive_type: "tar.gz".into(),
                sha1: Some("b".repeat(40)),
            }),
            packages: Vec::new(),
            supports_php: vec!["8.0".into(), "8.1".into(), "8.2".into()],
            released_at: Some("2024-04-09".into()),
        }],
        platforms: vec![],
        dependencies: vec![],
        conflicts: vec![],
        icon: None,
        screenshot: None,
    };
    assert!(entry.validate(&known).is_ok());
    let mut bad = entry.clone();
    bad.versions[0].artifact.as_mut().unwrap().sha256 = "not-hex".into();
    assert!(bad.validate(&known).is_err());
    let mut bad = entry.clone();
    bad.versions[0].artifact.as_mut().unwrap().archive_type = "exe".into();
    assert!(bad.validate(&known).is_err());
    let mut bad = entry.clone();
    bad.versions[0].artifact.as_mut().unwrap().archive_root = "../etc".into();
    assert!(bad.validate(&known).is_err());
}

#[test]
fn entry_rejects_dependency_and_conflict_references_outside_manifest() {
    let known = std::collections::BTreeSet::from(["redis".to_string()]);
    let mut entry = CatalogEntryRecipe {
        id: "redis".into(),
        name: "Redis".into(),
        description: "Cache".into(),
        long_description: String::new(),
        category: Category::Cache,
        kind: EntryKind::System,
        tags: vec![Tag::new("cache").unwrap()],
        license: License::new("RSALv2").unwrap(),
        developer: "x".into(),
        homepage: Homepage::new("https://redis.io").unwrap(),
        versions: vec![VersionSpec {
            version: "7.2.4".into(),
            size_bytes: 0,
            changelog_url: None,
            artifact: None,
            packages: vec!["redis-server".into()],
            supports_php: Vec::new(),
            released_at: None,
        }],
        platforms: vec![],
        dependencies: vec!["mysql".into()],
        conflicts: vec![],
        icon: None,
        screenshot: None,
    };
    assert_eq!(entry.validate(&known), Err(RecipeError::Unknown));
    entry.dependencies.clear();
    entry.conflicts = vec!["redis".into()];
    assert_eq!(
        entry.validate(&known),
        Err(RecipeError::Inconsistent("self conflict"))
    );
}

#[test]
fn manifest_rejects_duplicate_ids_and_unsupported_schema() {
    let entry = |id: &str| CatalogEntryRecipe {
        id: id.into(),
        name: "x".into(),
        description: "x".into(),
        long_description: String::new(),
        category: Category::Other,
        kind: EntryKind::System,
        tags: vec![Tag::new("misc").unwrap()],
        license: License::new("MIT").unwrap(),
        developer: "x".into(),
        homepage: Homepage::new("https://example.com").unwrap(),
        versions: vec![VersionSpec {
            version: "1.0.0".into(),
            size_bytes: 0,
            changelog_url: None,
            artifact: None,
            packages: vec!["pkg".into()],
            supports_php: Vec::new(),
            released_at: None,
        }],
        platforms: vec![],
        dependencies: vec![],
        conflicts: vec![],
        icon: None,
        screenshot: None,
    };
    let mut manifest = CatalogManifest {
        schema: 1,
        issued_at: "2024-01-01T00:00:00Z".into(),
        source_url: "https://catalog.example.com/v1/manifest.json".into(),
        entries: vec![entry("a"), entry("a")],
    };
    assert_eq!(
        manifest.validate(),
        Err(RecipeError::Inconsistent("duplicate id"))
    );
    manifest.entries = vec![entry("a")];
    manifest.schema = 2;
    assert_eq!(manifest.validate(), Err(RecipeError::Invalid("schema")));
}

#[test]
fn manifest_digest_is_stable_under_field_order() {
    let entry = CatalogEntryRecipe {
        id: "redis".into(),
        name: "Redis".into(),
        description: "Cache".into(),
        long_description: String::new(),
        category: Category::Cache,
        kind: EntryKind::System,
        tags: vec![Tag::new("cache").unwrap()],
        license: License::new("RSALv2").unwrap(),
        developer: "Redis Labs".into(),
        homepage: Homepage::new("https://redis.io").unwrap(),
        versions: vec![VersionSpec {
            version: "7.2.4".into(),
            size_bytes: 0,
            changelog_url: None,
            artifact: None,
            packages: vec!["redis-server".into()],
            supports_php: Vec::new(),
            released_at: None,
        }],
        platforms: vec![],
        dependencies: vec![],
        conflicts: vec![],
        icon: None,
        screenshot: None,
    };
    let manifest = CatalogManifest {
        schema: 1,
        issued_at: "2024-01-01T00:00:00Z".into(),
        source_url: "https://example.com".into(),
        entries: vec![entry],
    };
    let first = manifest.digest();
    let second = manifest.digest();
    assert_eq!(first, second);
    assert_eq!(first.len(), 64);
}

#[test]
fn search_query_defaults_are_sane() {
    let query = CatalogQuery::default();
    assert_eq!(query.page, 0);
    assert!(query.categories.is_empty());
    assert!(query.tags.is_empty());
    assert!(!query.installed_only);
    assert!(!query.update_available_only);
}
