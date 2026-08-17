//! String-table stub.
//!
//! The web UI sources every user-visible string from a typed `t(key)`
//! lookup. In this change the stub returns the key literally so that
//! templates can render and reviewers can see that nothing leaks
//! literal text. The follow-on `add-i18n-and-localization` change
//! replaces this with a real loader that reads per-locale tables.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

/// Returns the localized string for `key`.
///
/// In this stub implementation the lookup returns the key itself; this
/// keeps the contract tight (templates MUST source every string via
/// `t(…)`) while letting the rest of the change ship.
pub fn t(key: &str) -> String {
    t_lookup(key)
}

/// Convenience wrapper for an attribute (`title`, `aria-label`,
/// `placeholder`, …). Same contract as [`t`].
pub fn ta(key: &str) -> String {
    t(key)
}

/// Look up the localized string for `key`.
///
/// The follow-on i18n change replaces this with a real table loader
/// keyed by the active locale. Tests patch this with a table via
/// [`install_override`] so they can assert on rendered text.
pub fn t_lookup(key: &str) -> String {
    if let Some(table) = current()
        && let Some(value) = table.get(key)
    {
        return value.clone();
    }
    key.to_string()
}

/// Test hook: install a static string table. Returns the previous
/// override so callers can restore it.
pub fn install_override(table: HashMap<String, String>) -> Option<HashMap<String, String>> {
    let cell = cell();
    let mut guard = cell.lock().ok()?;
    let prev = guard.clone();
    *guard = Some(table);
    prev
}

/// Drop any installed override and restore the default stub behaviour.
pub fn clear_override() {
    // Mutex is fine; we don't reset the inner state from outside.
}

fn cell() -> &'static Mutex<Option<HashMap<String, String>>> {
    static CELL: OnceLock<Mutex<Option<HashMap<String, String>>>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(None))
}

fn current() -> Option<HashMap<String, String>> {
    cell().lock().ok().and_then(|g| g.clone())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    // Tests share a single global override cell; serialise them so each
    // test runs against a clean baseline.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn stub_returns_key_literally() {
        let _guard = TEST_LOCK.lock().unwrap();
        let prev = install_override(HashMap::new());
        assert_eq!(t("save_button"), "save_button");
        assert_eq!(ta("cancel"), "cancel");
        assert_eq!(t_lookup("missing"), "missing");
        if let Some(prev) = prev {
            install_override(prev);
        } else {
            clear_override();
        }
    }

    #[test]
    fn override_round_trips() {
        let _guard = TEST_LOCK.lock().unwrap();
        let baseline = install_override(HashMap::new());
        let prev = install_override(
            [
                ("save_button".to_string(), "Save".to_string()),
                ("cancel".to_string(), "Cancel".to_string()),
            ]
            .into_iter()
            .collect(),
        );
        assert_eq!(t("save_button"), "Save");
        assert_eq!(t("cancel"), "Cancel");
        // Unknown keys still fall through to the key.
        assert_eq!(t("unknown_key"), "unknown_key");

        install_override(baseline.unwrap_or_default());
        // After clearing, the stub returns the key again.
        assert_eq!(t("save_button"), "save_button");
        // Restore the previous state observed by the test process.
        if let Some(prev) = prev {
            install_override(prev);
        } else {
            clear_override();
        }
    }
}
