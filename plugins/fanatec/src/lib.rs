//! The Fanatec plugin: Fanatec wheel bases, rims and pedals on the hid-fanatecff driver, which
//! its driver (`driver/`) bundles and builds with DKMS. The adapter talks to the driver like a
//! game: sysfs attributes for settings, the event node for inputs and force feedback.

pub mod adapter;
pub mod buttons;
pub mod components;
pub mod device;
pub mod models;
pub mod tuning;
pub mod unlock;
