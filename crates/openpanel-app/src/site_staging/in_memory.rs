// Optional in-process helpers for the staging service. The
// filesystem layer's in-memory fake lives in `files.rs`.

/// Marker type reserved for future "promote completed" listeners
/// (e.g. a webhook, a notification channel). Production code wires
/// the real implementation at composition time; this stub keeps
/// the module addressable from the public `site_staging::` namespace
/// so the existing layout is stable.
pub struct InMemoryPromotionNotifier;
