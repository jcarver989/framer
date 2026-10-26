use std::io;
use std::path::PathBuf;
use thiserror::Error;
use toml_edit::TomlError;

#[derive(Debug, Error)]
pub enum FramerError {
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },

    #[error("{}: invalid TOML: {source}", path.display())]
    Toml { path: PathBuf, source: TomlError },

    #[error("{}: invalid manifest: {source}", path.display())]
    Manifest {
        path: PathBuf,
        source: serde_json::Error,
    },

    /// Two components tried to generate the same file, TOML key or block.
    #[error("{}: {message}", path.display())]
    Conflict { path: PathBuf, message: String },

    #[error("{}: {message}", path.display())]
    Invalid { path: PathBuf, message: String },
}

pub type Result<T, E = FramerError> = std::result::Result<T, E>;
