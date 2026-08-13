//! Docker aggregate safety-boundary tests.
#![allow(clippy::expect_used)]

use std::{collections::BTreeMap, path::PathBuf};

use openpanel_domain::docker::{
    BindMount, ComposeStack, ContainerSpec, ImageAllowlistEntry, PortBinding, ResourceLimits,
    RestartPolicy,
};
use proptest::prelude::*;
use uuid::Uuid;

fn valid() -> ContainerSpec {
    ContainerSpec {
        id: Uuid::new_v4(),
        name: "redis_site".into(),
        image: "library/redis:7".into(),
        site_id: None,
        env: BTreeMap::from([("REDIS_MODE".into(), "safe".into())]),
        command: vec![],
        ports: vec![PortBinding {
            host: 8080,
            container: 6379,
        }],
        mounts: vec![],
        capabilities: vec![],
        limits: ResourceLimits::default(),
        restart_policy: RestartPolicy::UnlessStopped,
        user_namespace: "1001:1001".into(),
    }
}

#[test]
fn defaults_and_valid_spec_pass() {
    valid().validate(8080..=8999).expect("valid spec");
}

#[test]
fn mount_outside_container_root_is_rejected() {
    let mut spec = valid();
    spec.mounts.push(BindMount {
        source: PathBuf::from("/etc"),
        target: PathBuf::from("/data"),
        read_only: true,
    });
    assert!(spec.validate(8080..=8999).is_err());
}

#[test]
fn compose_stack_requires_canonical_yaml_and_signature_envelope() {
    let parsed: serde_yaml::Value =
        serde_yaml::from_str("services:\n  redis:\n    image: redis:7\n").expect("yaml");
    let canonical = serde_yaml::to_string(&parsed).expect("canonical");
    let mut stack = ComposeStack {
        id: Uuid::new_v4(),
        name: "cache".into(),
        compose_yaml_canonical: canonical,
        signature_b64: "signature-envelope".into(),
        site_id: None,
    };
    stack.validate().expect("valid envelope");
    stack.compose_yaml_canonical = "services: {redis: {image: redis:7}}".into();
    assert!(stack.validate().is_err());
}

proptest! {
    #[test]
    fn every_forbidden_capability_is_rejected(index in 0usize..6) {
        let forbidden = ["cap_sys_admin", "cap_sys_ptrace", "cap_sys_module", "cap_net_admin", "cap_net_raw", "cap_dac_override"];
        let mut spec = valid(); spec.capabilities.push(forbidden[index].into());
        prop_assert!(spec.validate(8080..=8999).is_err());
    }

    #[test]
    fn arbitrary_outside_mount_never_passes(suffix in "[a-z]{1,16}") {
        let mut spec = valid(); spec.mounts.push(BindMount { source: PathBuf::from(format!("/tmp/{suffix}")), target: PathBuf::from("/data"), read_only: false });
        prop_assert!(spec.validate(8080..=8999).is_err());
    }

    #[test]
    fn every_image_outside_allowlist_is_rejected(
        namespace in "[a-z]{1,12}",
        image in "[a-z]{1,12}",
        tag in "[a-z0-9]{1,8}",
    ) {
        let entry = ImageAllowlistEntry {
            pattern: "library/redis:*".into(),
            allow_pull: true,
            pin_digest_required: false,
        };
        let candidate = format!("evil-{namespace}/{image}:{tag}");
        prop_assert!(!entry.matches(&candidate));
    }
}
