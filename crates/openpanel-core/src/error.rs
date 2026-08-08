use thiserror::Error;

/// Result alias for fallible core operations.
pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Error)]
/// Top-level error type for the core crate.
pub enum CoreError {
    /// A configuration error, wrapping [`ConfigError`].
    #[error("configuration error: {0}")]
    Config(#[from] ConfigError),

    /// A database error, wrapping [`DatabaseError`].
    #[error("database error: {0}")]
    Database(#[from] DatabaseError),

    /// An error raised by the sqlx driver layer.
    #[error("sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    /// A migration failed.
    #[error("migration error: {0}")]
    Migration(String),

    /// An I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// A module with the same name was registered twice.
    #[error("module `{0}` is already registered")]
    DuplicateModule(String),

    /// A lookup for a module that was never registered.
    #[error("module `{0}` not found")]
    UnknownModule(String),
}

#[derive(Debug, Error)]
/// Errors raised while loading or validating configuration.
pub enum ConfigError {
    /// The config could not be loaded or deserialized.
    #[error("could not load config: {0}")]
    Load(String),

    /// The config failed validation against the schema.
    #[error("config at `{path}` failed validation: {message}")]
    Validation {
        /// JSON Schema path of the invalid field.
        path: String,
        /// Human-readable validation failure message.
        message: String,
    },

    /// The schema could not be built.
    #[error("config schema merge failed: {0}")]
    Schema(String),
}

#[derive(Debug, Error)]
/// Errors raised by database operations.
pub enum DatabaseError {
    /// An error from the sqlx driver layer.
    #[error("sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    /// A migration failed while executing at a given path.
    #[error("migration failed at {path}: {message}")]
    Migration {
        /// `module/version` path identifying the failing migration.
        path: String,
        /// Underlying error message from the failed statement.
        message: String,
    },
}
