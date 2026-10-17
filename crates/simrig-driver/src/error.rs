use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("invalid {}: {message}", path.display())]
    Invalid { path: PathBuf, message: String },
    #[error("hook '{hook}' failed ({status}): {stderr}")]
    HookFailed {
        hook: String,
        status: String,
        stderr: String,
    },
    #[error("hook '{0}' timed out")]
    HookTimeout(String),
    #[error("no driver installed for plugin '{0}'")]
    NotInstalled(String),
    #[error("unknown privileged operation '{0}'")]
    UnknownOperation(String),
    #[error("argument '{name}': {message}")]
    InvalidArgument { name: String, message: String },
}
