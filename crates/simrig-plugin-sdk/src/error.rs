use simrig_model::{Unsupported, ValueError};
use simrig_protocol::{RpcError, code};

/// Error returned by adapter and interface methods.
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// The item is unsupported, with the reason shown to the user.
    Unsupported(Unsupported),
    /// The value does not fit the item.
    InvalidValue(String),
    /// Unknown device, component or item.
    NotFound(String),
    /// The device or driver failed.
    Failed(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// The default of every interface method: not supported by this driver.
    pub fn unsupported() -> Self {
        Error::Unsupported(Unsupported::not_in_driver())
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Error::Failed(message.into())
    }

    pub(crate) fn to_rpc(&self) -> RpcError {
        match self {
            Error::Unsupported(u) => RpcError::new(code::UNSUPPORTED, u.explain()).with_data(u),
            Error::InvalidValue(m) => RpcError::new(code::INVALID_VALUE, m.clone()),
            Error::NotFound(m) => RpcError::new(code::NOT_FOUND, m.clone()),
            Error::Failed(m) => RpcError::new(code::FAILED, m.clone()),
        }
    }
}

impl From<ValueError> for Error {
    fn from(e: ValueError) -> Self {
        Error::InvalidValue(e.0)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unsupported(u) => f.write_str(&u.explain()),
            Error::InvalidValue(m) | Error::NotFound(m) | Error::Failed(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for Error {}
