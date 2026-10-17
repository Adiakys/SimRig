use std::io;
use std::path::PathBuf;

use simrig_model::Unsupported;
use simrig_protocol::{RpcError, code};

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("invalid plugin manifest {}: {message}", path.display())]
    Manifest { path: PathBuf, message: String },
    #[error("{}", .0.explain())]
    Unsupported(Unsupported),
    #[error("invalid value: {0}")]
    InvalidValue(String),
    #[error("{0}")]
    NotFound(String),
    #[error("plugin '{0}' is not running")]
    PluginNotRunning(String),
    #[error("plugin '{plugin}' did not answer '{method}' in time")]
    Timeout { plugin: String, method: String },
    #[error("plugin '{0}' exited")]
    PluginExited(String),
    #[error("plugin '{plugin}': {error}")]
    Plugin { plugin: String, error: RpcError },
}

impl HostError {
    pub(crate) fn from_rpc(plugin: &str, error: RpcError) -> Self {
        match error.code {
            code::UNSUPPORTED => match error
                .data
                .clone()
                .and_then(|d| serde_json::from_value(d).ok())
            {
                Some(u) => HostError::Unsupported(u),
                None => HostError::Plugin {
                    plugin: plugin.into(),
                    error,
                },
            },
            code::INVALID_VALUE => HostError::InvalidValue(error.message),
            code::NOT_FOUND => HostError::NotFound(error.message),
            _ => HostError::Plugin {
                plugin: plugin.into(),
                error,
            },
        }
    }
}
