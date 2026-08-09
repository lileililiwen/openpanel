#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use openpanel_app::system_services::{
    ControllerStatus, HealthSupervisor, JournalPort, ServiceController, ServiceHealthRepository,
    ServiceManager, ServiceManagerError,
};
use openpanel_domain::{
    Role,
    system_services::{ServiceAction, ServiceDescriptor, ServiceId},
};
use openpanel_test_support::MockAudit;
use uuid::Uuid;

mockall::mock! {
    Controller {}
    #[async_trait]
    impl ServiceController for Controller {
        async fn status(&self, unit: &str) -> Result<ControllerStatus, ServiceManagerError>;
        async fn action(&self, unit: &str, action: ServiceAction) -> Result<(), ServiceManagerError>;
        async fn probe(&self, unit: &str) -> Result<bool, ServiceManagerError>;
    }
}
mockall::mock! {
    Journal {}
    #[async_trait]
    impl JournalPort for Journal {
        async fn entries(&self, unit: &str, limit: usize) -> Result<Vec<String>, ServiceManagerError>;
    }
}
mockall::mock! {
    HealthRepo {}
    #[async_trait]
    impl ServiceHealthRepository for HealthRepo {
        async fn history(&self, service_id: &str, limit: usize) -> Result<Vec<openpanel_domain::system_services::Health>, ServiceManagerError>;
        async fn record(&self, service_id: &str, health: openpanel_domain::system_services::Health) -> Result<(), ServiceManagerError>;
    }
}

#[tokio::test]
async fn persistent_failure_records_one_transition_and_bounded_auto_restart() {
    let mut controller = MockController::new();
    controller.expect_probe().times(3).returning(|_| Ok(false));
    controller
        .expect_action()
        .once()
        .withf(|unit, action| unit == "nginx.service" && *action == ServiceAction::Restart)
        .returning(|_, _| Ok(()));
    let mut repo = MockHealthRepo::new();
    repo.expect_record()
        .once()
        .withf(|id, health| {
            id == "nginx" && *health == openpanel_domain::system_services::Health::Failed
        })
        .returning(|_, _| Ok(()));
    let supervisor = HealthSupervisor::new(
        vec![nginx()],
        Arc::new(controller),
        Arc::new(repo),
        Arc::new(MockAudit::stub()),
        2,
        true,
        1,
        30,
    )
    .unwrap();
    supervisor.tick(100).await.unwrap();
    supervisor.tick(101).await.unwrap();
    supervisor.tick(102).await.unwrap();
}

fn nginx() -> ServiceDescriptor {
    ServiceDescriptor::new(
        ServiceId::new("nginx").unwrap(),
        "Nginx",
        "nginx.service",
        vec![
            ServiceAction::Start,
            ServiceAction::Stop,
            ServiceAction::Restart,
            ServiceAction::Reload,
        ],
        vec!["sites".into(), "ssl".into()],
    )
    .unwrap()
}

#[tokio::test]
async fn unknown_id_runs_no_controller_command() {
    let mut controller = MockController::new();
    controller.expect_status().never();
    controller.expect_action().never();
    let manager = ServiceManager::new(
        vec![nginx()],
        Arc::new(controller),
        Arc::new(MockJournal::new()),
        Arc::new(MockHealthRepo::new()),
        Arc::new(MockAudit::stub()),
    );
    assert!(matches!(
        manager.status("ssh").await,
        Err(ServiceManagerError::NotFound)
    ));
}

#[tokio::test]
async fn restart_requires_owner_confirmation_then_uses_fixed_unit_and_checks_readiness() {
    let mut controller = MockController::new();
    controller
        .expect_action()
        .once()
        .withf(|unit, action| unit == "nginx.service" && *action == ServiceAction::Restart)
        .returning(|_, _| Ok(()));
    controller
        .expect_probe()
        .once()
        .withf(|unit| unit == "nginx.service")
        .returning(|_| Ok(true));
    controller
        .expect_status()
        .once()
        .returning(|_| Ok(openpanel_app::system_services::ControllerStatus::active()));
    let manager = ServiceManager::new(
        vec![nginx()],
        Arc::new(controller),
        Arc::new(MockJournal::new()),
        Arc::new(MockHealthRepo::new()),
        Arc::new(MockAudit::stub()),
    );
    assert!(matches!(
        manager
            .perform(
                Uuid::new_v4(),
                Role::Owner,
                "nginx",
                ServiceAction::Restart,
                false
            )
            .await,
        Err(ServiceManagerError::ConfirmationRequired)
    ));
    assert!(
        manager
            .perform(
                Uuid::new_v4(),
                Role::Owner,
                "nginx",
                ServiceAction::Restart,
                true
            )
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn admin_can_reload_but_cannot_restart_and_journal_is_bounded_redacted() {
    let mut controller = MockController::new();
    controller.expect_action().once().returning(|_, _| Ok(()));
    controller.expect_probe().once().returning(|_| Ok(true));
    controller
        .expect_status()
        .once()
        .returning(|_| Ok(openpanel_app::system_services::ControllerStatus::active()));
    let mut journal = MockJournal::new();
    journal
        .expect_entries()
        .once()
        .withf(|unit, limit| unit == "nginx.service" && *limit == 2)
        .returning(|_, _| Ok(vec!["password=hunter2".into(), "ready".into()]));
    let manager = ServiceManager::new(
        vec![nginx()],
        Arc::new(controller),
        Arc::new(journal),
        Arc::new(MockHealthRepo::new()),
        Arc::new(MockAudit::stub()),
    );
    assert!(matches!(
        manager
            .perform(
                Uuid::new_v4(),
                Role::Admin,
                "nginx",
                ServiceAction::Restart,
                true
            )
            .await,
        Err(ServiceManagerError::Forbidden)
    ));
    manager
        .perform(
            Uuid::new_v4(),
            Role::Admin,
            "nginx",
            ServiceAction::Reload,
            false,
        )
        .await
        .unwrap();
    let entries = manager.logs("nginx", 2).await.unwrap();
    assert!(!entries.join(" ").contains("hunter2"));
}
