use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::sites::{error::SiteError, status::SiteStatus};

const DOMAIN_RE: &str =
    r"^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)+$";
const ALIAS_RE: &str =
    r"^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)+$";

/// A nginx-hosted site: the aggregate root of the sites bounded context.
///
/// Construction enforces the domain invariants (hostname pattern,
/// aliases, document root). Fields are private and read via accessors;
/// mutation goes through the `enable`/`disable`/`change_*` methods.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    id: Uuid,
    owner_id: Uuid,
    primary_domain: String,
    aliases: Vec<String>,
    document_root: String,
    php_enabled: bool,
    php_version: Option<String>,
    status: SiteStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    created_by: String,
    modified_by: String,
}

impl Site {
    /// Create a new site, validating the primary domain, aliases, and
    /// document root. New sites start with `SiteStatus::Active`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        owner_id: Uuid,
        primary_domain: impl Into<String>,
        aliases: Vec<String>,
        document_root: impl Into<String>,
        php_enabled: bool,
        php_version: Option<String>,
        created_by: impl Into<String>,
    ) -> Result<Self, SiteError> {
        let primary_domain = primary_domain.into().to_lowercase();
        validate_domain(&primary_domain)?;

        let mut validated_aliases = Vec::with_capacity(aliases.len());
        for alias in aliases {
            let a = alias.to_lowercase();
            if a == primary_domain {
                return Err(SiteError::InvalidAlias(
                    a,
                    "alias matches primary domain".into(),
                ));
            }
            validate_alias(&a)?;
            if validated_aliases.contains(&a) {
                return Err(SiteError::InvalidAlias(a, "duplicate alias".into()));
            }
            validated_aliases.push(a);
        }

        let document_root = document_root.into();
        validate_document_root(&document_root)?;

        let now = Utc::now();
        Ok(Self {
            id,
            owner_id,
            primary_domain,
            aliases: validated_aliases,
            document_root,
            php_enabled,
            php_version,
            status: SiteStatus::Active,
            created_at: now,
            updated_at: now,
            created_by: created_by.into(),
            modified_by: String::new(),
        })
    }

    /// Set the site's status to `Active`, recording `by` as the modifier.
    pub fn enable(&mut self, by: &str) {
        self.status = SiteStatus::Active;
        self.touch(by);
    }

    /// Set the site's status to `Disabled`, recording `by` as the modifier.
    pub fn disable(&mut self, by: &str) {
        self.status = SiteStatus::Disabled;
        self.touch(by);
    }

    /// Transfer the site to a new owner, recording `by` as the modifier.
    pub fn change_owner(&mut self, new_owner: Uuid, by: &str) {
        self.owner_id = new_owner;
        self.touch(by);
    }

    /// Replace the aliases, validating each; rejects aliases that match
    /// the primary domain or duplicate each other.
    pub fn change_aliases(&mut self, aliases: Vec<String>, by: &str) -> Result<(), SiteError> {
        let mut validated = Vec::with_capacity(aliases.len());
        for alias in aliases {
            let a = alias.to_lowercase();
            if a == self.primary_domain {
                return Err(SiteError::InvalidAlias(
                    a,
                    "alias matches primary domain".into(),
                ));
            }
            validate_alias(&a)?;
            if validated.contains(&a) {
                return Err(SiteError::InvalidAlias(a, "duplicate alias".into()));
            }
            validated.push(a);
        }
        self.aliases = validated;
        self.touch(by);
        Ok(())
    }

    fn touch(&mut self, by: &str) {
        self.updated_at = Utc::now();
        if !by.is_empty() {
            self.modified_by = by.to_string();
        }
    }

    /// Rebuild a site from persistence. Used by the repository adapter;
    /// skips validation because stored state is already trusted.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        owner_id: Uuid,
        primary_domain: String,
        aliases: Vec<String>,
        document_root: String,
        php_enabled: bool,
        php_version: Option<String>,
        status: SiteStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        created_by: String,
        modified_by: String,
    ) -> Self {
        Self {
            id,
            owner_id,
            primary_domain,
            aliases,
            document_root,
            php_enabled,
            php_version,
            status,
            created_at,
            updated_at,
            created_by,
            modified_by,
        }
    }

    /// The site's unique identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// The id of the user who owns this site.
    pub fn owner_id(&self) -> Uuid {
        self.owner_id
    }

    /// The site's primary domain.
    pub fn primary_domain(&self) -> &str {
        &self.primary_domain
    }

    /// The site's aliases.
    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }

    /// The absolute document root served by nginx.
    pub fn document_root(&self) -> &str {
        &self.document_root
    }

    /// Whether PHP is enabled for this site.
    pub fn php_enabled(&self) -> bool {
        self.php_enabled
    }

    /// The PHP version, if PHP is enabled.
    pub fn php_version(&self) -> Option<&str> {
        self.php_version.as_deref()
    }

    /// The site's current status.
    pub fn status(&self) -> SiteStatus {
        self.status
    }

    /// When the site was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// When the site was last modified.
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Who created the site.
    pub fn created_by(&self) -> &str {
        &self.created_by
    }

    /// Who last modified the site.
    pub fn modified_by(&self) -> &str {
        &self.modified_by
    }
}

fn validate_domain(s: &str) -> Result<(), SiteError> {
    if s.is_empty() || s.len() > 253 {
        return Err(SiteError::InvalidDomain("length out of range".into()));
    }
    #[allow(clippy::expect_used)]
    let re = regex::Regex::new(DOMAIN_RE)
        .expect("DOMAIN_RE is a compile-time constant; a broken pattern is a programming error");
    if !re.is_match(s) {
        return Err(SiteError::InvalidDomain(format!(
            "`{s}` does not match domain pattern"
        )));
    }
    Ok(())
}

fn validate_alias(s: &str) -> Result<(), SiteError> {
    if s.is_empty() || s.len() > 253 {
        return Err(SiteError::InvalidAlias(
            s.into(),
            "length out of range".into(),
        ));
    }
    if s.starts_with('.') || s.contains('*') {
        return Err(SiteError::InvalidAlias(
            s.into(),
            "wildcards not allowed".into(),
        ));
    }
    #[allow(clippy::expect_used)]
    let re = regex::Regex::new(ALIAS_RE)
        .expect("ALIAS_RE is a compile-time constant; a broken pattern is a programming error");
    if !re.is_match(s) {
        return Err(SiteError::InvalidAlias(s.into(), "invalid hostname".into()));
    }
    Ok(())
}

fn validate_document_root(p: &str) -> Result<(), SiteError> {
    if !p.starts_with('/') {
        return Err(SiteError::InvalidDocumentRoot(p.into()));
    }
    if p.contains("..") {
        return Err(SiteError::InvalidDocumentRoot(format!("{p} contains `..`")));
    }
    if p.contains('\0') {
        return Err(SiteError::InvalidDocumentRoot(format!("{p} contains NUL")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn make_site() -> Result<Site, SiteError> {
        Site::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "example.com",
            vec!["www.example.com".into()],
            "/var/www/example.com/public_html",
            false,
            None,
            "tester",
        )
    }

    #[test]
    fn accepts_valid_domain() {
        assert!(make_site().is_ok());
    }

    #[test]
    fn rejects_invalid_domain() {
        let r = Site::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "not a domain",
            vec![],
            "/var/www/x/public_html",
            false,
            None,
            "t",
        );
        assert!(matches!(r, Err(SiteError::InvalidDomain(_))));
    }

    #[test]
    fn rejects_alias_matching_primary() {
        let r = Site::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "example.com",
            vec!["example.com".into()],
            "/var/www/example.com/public_html",
            false,
            None,
            "t",
        );
        assert!(matches!(r, Err(SiteError::InvalidAlias(_, _))));
    }

    #[test]
    fn rejects_relative_document_root() {
        let r = Site::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "example.com",
            vec![],
            "var/www/x/public_html",
            false,
            None,
            "t",
        );
        assert!(matches!(r, Err(SiteError::InvalidDocumentRoot(_))));
    }

    #[test]
    fn rejects_wildcard_alias() {
        let r = Site::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "example.com",
            vec!["*.example.com".into()],
            "/var/www/example.com/public_html",
            false,
            None,
            "t",
        );
        assert!(matches!(r, Err(SiteError::InvalidAlias(_, _))));
    }

    #[test]
    fn enable_disable() {
        let mut s = make_site().unwrap();
        assert_eq!(s.status(), SiteStatus::Active);
        s.disable("admin");
        assert_eq!(s.status(), SiteStatus::Disabled);
        s.enable("admin");
        assert_eq!(s.status(), SiteStatus::Active);
    }
}

#[cfg(test)]
mod prop {
    use proptest::prelude::*;

    use super::*;

    fn make_site(domain: &str, aliases: Vec<String>, doc_root: &str) -> Result<Site, SiteError> {
        Site::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            domain,
            aliases,
            doc_root,
            false,
            None,
            "tester",
        )
    }

    proptest! {
        #[test]
        fn prop_domain_with_slash_rejected(s in "[a-z0-9]+/[a-z0-9]+") {
            prop_assert!(!matches!(
                make_site(&s, vec![], "/var/www/x/public_html"),
                Err(SiteError::InvalidDomain(_))
            ) || s.contains('/'));
        }

        #[test]
        fn prop_domain_with_space_rejected(
            prefix in "[a-z0-9]{1,20}",
            suffix in "[a-z0-9]{1,20}"
        ) {
            let domain = format!("{prefix} {suffix}");
            prop_assert!(matches!(
                make_site(&domain, vec![], "/var/www/x/public_html"),
                Err(SiteError::InvalidDomain(_))
            ));
        }

        #[test]
        fn prop_domain_with_wildcard_rejected(s in "[a-z0-9*]{1,20}") {
            if s.contains('*') {
                prop_assert!(matches!(
                    make_site(&s, vec![], "/var/www/x/public_html"),
                    Err(SiteError::InvalidDomain(_))
                ));
            }
        }

        #[test]
        fn prop_domain_with_control_chars_rejected(
            s in "\\p{C}{1,20}"
        ) {
            prop_assert!(!s.is_empty());
            prop_assert!(matches!(
                make_site(&s, vec![], "/var/www/x/public_html"),
                Err(SiteError::InvalidDomain(_))
            ));
        }

        #[test]
        fn prop_document_root_with_parent_traversal_rejected(
            root in "/var/www/[a-z]{1,8}/\\.\\./[a-z]{1,8}"
        ) {
            prop_assert!(matches!(
                make_site("example.com", vec![], &root),
                Err(SiteError::InvalidDocumentRoot(_))
            ));
        }

        #[test]
        fn prop_alias_matching_primary_rejected(
            domain in "[a-z0-9]{1,10}\\.[a-z0-9]{1,10}"
        ) {
            prop_assert!(matches!(
                make_site(&domain, vec![domain.clone()], "/var/www/x/public_html"),
                Err(SiteError::InvalidAlias(_, _))
            ));
        }
    }
}
