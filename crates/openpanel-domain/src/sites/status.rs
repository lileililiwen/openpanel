use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

/// The lifecycle status of a site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteStatus {
    /// The site is enabled and its vhost is live.
    Active,

    /// The site is disabled and its vhost is not served.
    Disabled,
}

impl SiteStatus {
    /// The canonical lowercase string form used for storage and display.
    pub fn as_str(&self) -> &'static str {
        match self {
            SiteStatus::Active => "active",
            SiteStatus::Disabled => "disabled",
        }
    }
}

impl fmt::Display for SiteStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SiteStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(SiteStatus::Active),
            "disabled" => Ok(SiteStatus::Disabled),
            other => Err(format!("unknown status `{other}`")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        assert_eq!(SiteStatus::Active.as_str(), "active");
        assert_eq!(
            "disabled".parse::<SiteStatus>().unwrap(),
            SiteStatus::Disabled
        );
    }
}
