use thiserror::Error;

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migration(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("module `{0}` is already registered")]
    DuplicateModule(String),

    #[error("module `{0}` not found")]
    UnknownModule(String),
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not load config: {0}")]
    Load(String),

    #[error("config at `{path}` failed validation: {message}")]
    Validation { path: String, message: String },

    #[error("config schema merge failed: {0}")]
    Schema(String),
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("migration failed at {path}: {message}")]
    Migration { path: String, message: String },
}