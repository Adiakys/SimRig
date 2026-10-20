//! One Fanatec device attached to the plugin: where hid-fanatecff exposes it (the HID device
//! bound to the `fanatec` driver, its tuning menu and mounted rim) and how it is read.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use simrig_plugin_kit::device::{Binding, HasHid, HidDevice};
use simrig_plugin_kit::pedals::Layout;
use simrig_plugin_kit::sysfs;
use simrig_plugin_sdk::prelude::*;

use std::time::Instant;

use crate::models::{Kind, Model};
use crate::unlock::{Menu, Slots, Unlock};

pub const BINDING: Binding = Binding {
    vendor: 0x0eb7,
    drivers: &["fanatec"],
    // A base has several HID devices bound to the driver; the one with its settings and its
    // inputs is the one to talk to (the others only carry raw HID).
    rich: &["range", "input"],
    unbound: "The hid-fanatec driver is not installed, or not bound to the device.",
};

/// What to do when the driver has not received the tuning menu from the base.
pub const TUNING_UNREAD: &str = "The base has not sent its settings to the driver yet. SimRig \
     asks for them; if they do not arrive, open the tuning menu on the wheel and change any \
     value (then set it back).";

/// The pedals plugged into a base: throttle ABS_Z, brake ABS_RZ, clutch ABS_Y, at the top of
/// their travel when released (checked on a CSL DD; the same table as Oversteer's).
pub const BASE_PEDALS: Layout = &[
    (PedalRole::Throttle, 0x02, true),
    (PedalRole::Brake, 0x05, true),
    (PedalRole::Clutch, 0x01, true),
];

pub struct FanatecDevice {
    pub hid: HidDevice,
    pub model: &'static Model,
    /// Getting the tuning menu from the base, while the driver has not received it.
    unlock: Unlock,
    slots: Slots,
}

pub type Shared = Rc<RefCell<FanatecDevice>>;

impl HasHid for FanatecDevice {
    fn hid(&self) -> &HidDevice {
        &self.hid
    }
    fn hid_mut(&mut self) -> &mut HidDevice {
        &mut self.hid
    }
}

impl FanatecDevice {
    pub fn new(id: &str, model: &'static Model, usb: PathBuf, config: PathBuf) -> Self {
        let base = matches!(model.kind, Kind::Base { .. });
        let mut hid = HidDevice::new(
            id,
            usb,
            BINDING,
            config,
            &format!("{:04x}", model.pid),
            base.then_some(0),
        );
        if base {
            hid = hid.with_layout(BASE_PEDALS);
        }
        let slots = Slots::for_config(&hid.config);
        Self {
            hid,
            model,
            unlock: Unlock::Idle,
            slots,
        }
    }

    /// An entry of the tuning menu (`ftec_tuning/<name>/<entry>`), when the base has it.
    pub fn tuning(&self, name: &str) -> Option<PathBuf> {
        let dir = sysfs::first_dir(&self.hid.hid.as_ref()?.join("ftec_tuning"))?;
        let path = dir.join(name);
        path.exists().then_some(path)
    }

    /// Asks the base for its tuning menu while the driver has not received it, and remembers
    /// the slot in use once it has (switching the menu's mode selects slot 1: the slot is put
    /// back). Called at every poll.
    pub fn tick(&mut self) {
        let Some(dir) = self
            .hid
            .hid
            .as_ref()
            .and_then(|h| sysfs::first_dir(&h.join("ftec_tuning")))
        else {
            // Unbound, or no tuning menu: start over when it comes.
            self.unlock = Unlock::Idle;
            return;
        };
        let menu = Menu { dir: &dir };
        let key = format!("{:04x}", self.model.pid);
        self.unlock = self
            .unlock
            .step(&menu, Instant::now(), self.slots.get(&key));
        if self.unlock == Unlock::Done {
            if let Some(slot) = sysfs::read_int(&dir.join("SLOT")).filter(|s| *s != 0) {
                self.slots.set(&key, slot);
            }
        }
    }

    /// The base has a tuning menu that the driver has not received yet: the base sends it only
    /// when the menu is used on the wheel (hid-fanatecff has no way to ask for it), and until
    /// then the driver refuses every write to it (the rotation included, which goes through
    /// the menu's SEN entry).
    pub fn tuning_unread(&self) -> Option<Unsupported> {
        let slot = sysfs::read_int(&self.tuning("SLOT")?)?;
        (slot == 0).then(|| Unsupported::new(Reason::Unavailable).with_detail(TUNING_UNREAD))
    }

    /// The mounted rim's directory (`ftec_wheel/<name>/`), when the driver knows it.
    pub fn wheel_dir(&self) -> Option<PathBuf> {
        sysfs::first_dir(&self.hid.hid.as_ref()?.join("ftec_wheel"))
    }

    /// The mounted rim's id; `None` with no rim (or no driver).
    pub fn wheel_id(&self) -> Option<u16> {
        let id = self.hid.read_int("wheel_id")?;
        (id != 0).then_some(id as u16)
    }

    /// The rotation the driver applies; its "automatic" value is the base's maximum.
    pub fn range(&self) -> Option<f64> {
        let range = self.hid.read_int("range")? as f64;
        Some(range.min(self.max_range()))
    }

    pub fn max_range(&self) -> f64 {
        match self.model.kind {
            Kind::Base { max_range } => f64::from(max_range),
            Kind::Pedals => 0.0,
        }
    }
}
