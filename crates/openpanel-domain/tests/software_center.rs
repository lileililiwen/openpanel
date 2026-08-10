#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use openpanel_domain::software_center::{
    ArchivePolicy, CatalogId, ConfirmationToken, DependencyGraph, JobState, PackageId, PlanAction,
    SoftwareJob, SoftwarePlan, SoftwareVersion, SupportedPlatform,
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
}
