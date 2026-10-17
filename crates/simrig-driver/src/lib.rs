//! The contract between simrig and a plugin's driver.
//!
//! A driver is independent of the app and may be built however it wants. simrig only knows how
//! to install, check and remove it, and which privileged operations its adapter may request,
//! as declared in `driver.toml`. Installed drivers are recorded in a registry together with a
//! copy of the driver directory, so they can be removed even after the app is gone.

mod contract;
mod error;
mod hooks;
mod layout;
mod ops;
mod registry;
#[cfg(feature = "testing")]
pub mod testing;

pub use contract::{DriverContract, DriverStatus, PrivilegedOp};
pub use error::DriverError;
pub use hooks::{HookContext, HookOutput, run_hook};
pub use layout::{SYSTEM_STATE_DIR, StateLayout, effective_uid};
pub use ops::{install, privileged, uninstall};
pub use registry::{InstallState, InstalledDriver, Registry};
