//! Host-operations bounded context: capability-guarded dispatch and reversible cutover.

pub mod cutover;

pub use crate::agent::{HostAdapter, HostAdapterService, MemoryHostAdapter};
pub use cutover::{CutoverEvidence, CutoverService, DnsCutover, MailCutover, SslCutover};
