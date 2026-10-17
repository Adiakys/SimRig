//! The Thrustmaster plugin: Thrustmaster wheels on hid-tmff2, which its driver (`driver/`)
//! bundles and builds with DKMS (the kernel only starts the wheels up, without force feedback).

pub mod adapter;
pub mod components;
pub mod device;
pub mod models;
