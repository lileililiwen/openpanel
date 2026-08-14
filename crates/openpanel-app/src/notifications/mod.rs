//! Durable SMTP and signed-webhook notifications.

pub mod adapter;
pub mod module;
pub mod repo;
pub mod service;
pub mod task;

pub use adapter::{AdapterOutcome, NotificationAdapter, RustlsNotificationAdapter};
pub use module::NotificationModule;
pub use repo::SqliteNotificationRepository;
pub use service::{
    CreateSmtpChannel, CreateSubscription, CreateWebhookChannel, DeliveryView, NotificationService,
    NotificationServiceError, SubscriptionView,
};
pub use task::NotificationDispatcherTask;
