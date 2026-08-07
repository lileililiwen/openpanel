use std::path::Component;

use crate::files::error::FileError;

/// A relative path inside a site's document root.
///
/// `Path::new` enforces the invariants:
/// - must not be absolute (no leading `/`)
/// - must not contain `..` components
/// - must not contain null bytes
///
/// Use `Path::root()` to refer to the document root itself
/// (returned by listing operations).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path(String);

impl Path {
    /// Create a new `Path` from a user-supplied relative path. Returns
    /// `FileError::InvalidPath` on any invariant violation.
    pub fn new(input: impl Into<String>) -> Result<Self, FileError> {
        let s: String = input.into();
        Self::validate(&s)?;
        Ok(Self(s))
    }

    /// The "root" sentinel — represents the document root directory.
    pub fn root() -> Self {
        Self(String::new())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    fn validate(s: &str) -> Result<(), FileError> {
        if s.is_empty() {
            return Ok(());
        }
        if s.contains('\0') {
            return Err(FileError::InvalidPath("contains null byte".into()));
        }
        if s.starts_with('/') {
            return Err(FileError::InvalidPath("absolute path not allowed".into()));
        }
        let path = std::path::Path::new(s);
        for comp in path.components() {
            match comp {
                Component::ParentDir => {
                    return Err(FileError::InvalidPath("`..` not allowed".into()));
                }
                Component::RootDir => {
                    return Err(FileError::InvalidPath("absolute path not allowed".into()));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Append a basename to this path. The basename is itself validated.
    pub fn join(&self, basename: &str) -> Result<Self, FileError> {
        if basename.is_empty() {
            return Err(FileError::InvalidPath("empty basename".into()));
        }
        if basename.contains('/') || basename.contains('\0') {
            return Err(FileError::InvalidPath("invalid basename".into()));
        }
        if self.is_root() {
            Self::new(basename)
        } else {
            Self::new(format!("{}/{}", self.0, basename))
        }
    }
}

impl std::fmt::Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_simple_relative() {
        assert!(Path::new("index.html").is_ok());
        assert!(Path::new("wp-content/themes/2021/style.css").is_ok());
    }

    #[test]
    fn accepts_root() {
        let p = Path::root();
        assert!(p.is_root());
    }

    #[test]
    fn rejects_absolute() {
        let r = Path::new("/etc/passwd");
        assert!(matches!(r, Err(FileError::InvalidPath(_))));
    }

    #[test]
    fn rejects_parent_traversal() {
        assert!(matches!(
            Path::new("../etc/passwd"),
            Err(FileError::InvalidPath(_))
        ));
        assert!(matches!(
            Path::new("a/../../etc"),
            Err(FileError::InvalidPath(_))
        ));
    }

    #[test]
    fn rejects_null_byte() {
        assert!(matches!(
            Path::new("foo\0bar"),
            Err(FileError::InvalidPath(_))
        ));
    }

    #[test]
    fn join_appends() {
        let p = Path::new("wp-content").unwrap();
        let joined = p.join("themes").unwrap();
        assert_eq!(joined.as_str(), "wp-content/themes");
    }
}