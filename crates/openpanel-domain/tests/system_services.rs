#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use openpanel_domain::system_services::{
    AutoRestartBudget, Health, HealthTracker, ServiceAction, ServiceDescriptor, ServiceId,
};
use proptest::prelude::*;

#[test]
fn descriptors_are_fixed_allowlists_with_typed_actions_and_impacts() {
    let descriptor = ServiceDescriptor::new(
        ServiceId::new("nginx").unwrap(),
        "Nginx",
        "nginx.service",
        vec![
            ServiceAction::Start,
            ServiceAction::Restart,
            ServiceAction::Reload,
        ],
        vec!["sites".into(), "ssl".into()],
    )
    .unwrap();
    assert_eq!(descriptor.unit(), "nginx.service");
    assert!(descriptor.supports(ServiceAction::Reload));
    assert!(!descriptor.supports(ServiceAction::Disable));
    assert_eq!(descriptor.dependent_capabilities(), &["sites", "ssl"]);
    assert!(ServiceId::new("../../ssh.service").is_err());
}

#[test]
fn health_hysteresis_emits_one_failure_and_one_recovery_transition() {
    let mut tracker = HealthTracker::new(2).unwrap();
    assert_eq!(tracker.observe(false), None);
    assert_eq!(tracker.observe(false), Some(Health::Failed));
    assert_eq!(tracker.observe(false), None);
    assert_eq!(tracker.observe(true), None);
    assert_eq!(tracker.observe(true), Some(Health::Healthy));
}

#[test]
fn auto_restart_budget_enforces_attempt_limit_and_cooldown() {
    let mut budget = AutoRestartBudget::new(2, 30).unwrap();
    assert!(budget.try_acquire(100));
    assert!(!budget.try_acquire(110));
    assert!(budget.try_acquire(130));
    assert!(!budget.try_acquire(160));
}

proptest! {
    #[test]
    fn prop_client_ids_cannot_become_systemd_units(value in ".*") {
        if let Ok(id) = ServiceId::new(&value) {
            prop_assert!(!id.as_str().contains('.'));
            prop_assert!(!id.as_str().contains('/'));
        }
    }

    #[test]
    fn prop_hysteresis_transition_count_is_bounded(values in proptest::collection::vec(any::<bool>(), 0..200)) {
        let mut tracker = HealthTracker::new(3).unwrap();
        let transitions = values.into_iter().filter_map(|healthy| tracker.observe(healthy)).count();
        prop_assert!(transitions <= 67);
    }


    #[test]
    fn prop_restart_attempts_never_exceed_budget(times in proptest::collection::vec(0_u64..1000, 0..300)) {
        let mut budget = AutoRestartBudget::new(3, 10).unwrap();
        let accepted = times.into_iter().filter(|time| budget.try_acquire(*time)).count();
        prop_assert!(accepted <= 100);
    }
}
