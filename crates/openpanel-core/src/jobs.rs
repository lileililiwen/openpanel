//! Background job supervisor. Each module returns a list of long-running
//! tasks; the agent spawns them with a shutdown signal. `tokio::spawn`
//! already catches panics via `JoinHandle::await`, so no manual
//! `catch_unwind` is needed.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

#[async_trait]
pub trait BackgroundTask: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    async fn run(self: Box<Self>, shutdown: Arc<Notify>) -> anyhow::Result<()>;
}

/// Cron-style recurring task. Used by future modules (cron, monitoring).
/// v0.1 only stubs the trait.
#[allow(async_fn_in_trait)]
pub trait Job: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn schedule(&self) -> Duration;
    async fn tick(&self) -> anyhow::Result<()>;
}

pub struct JobSupervisor {
    shutdown: Arc<Notify>,
}

impl JobSupervisor {
    pub fn new() -> Self {
        Self {
            shutdown: Arc::new(Notify::new()),
        }
    }

    pub fn spawn(self, tasks: Vec<Box<dyn BackgroundTask>>) -> SupervisorHandle {
        let shutdown = self.shutdown.clone();
        let mut handles = Vec::with_capacity(tasks.len());
        for task in tasks {
            let name = task.name();
            let sd = shutdown.clone();
            let handle = tokio::spawn(async move {
                let result = std::panic::AssertUnwindSafe(task.run(sd)).await;
                match result {
                    Ok(()) => tracing::info!(name, "background task exited cleanly"),
                    Err(e) => tracing::error!(name, error = %e, "background task failed"),
                }
            });
            handles.push(handle);
        }
        SupervisorHandle { shutdown, handles }
    }
}

impl Default for JobSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SupervisorHandle {
    shutdown: Arc<Notify>,
    handles: Vec<JoinHandle<()>>,
}

impl SupervisorHandle {
    pub fn shutdown_handle(&self) -> Arc<Notify> {
        self.shutdown.clone()
    }

    pub async fn join(self) {
        self.shutdown.notify_waiters();
        for h in self.handles {
            let _ = h.await;
        }
    }
}
