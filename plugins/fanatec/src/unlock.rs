//! Getting the tuning menu from a base that has not sent it.
//!
//! A base sends its tuning menu to hid-fanatecff only when the menu changes, and until it has
//! the driver refuses to change it, the rotation included (gotzl/hid-fanatecff#75). Switching
//! the menu between its standard and advanced mode is a change the driver lets through
//! (`advanced_mode`): the base answers with the whole menu. So the menu is switched, then
//! switched back once it has arrived, and the slot the user had is selected again (switching
//! modes goes back to slot 1). Checked on a CSL DD: the base answers in about 200 ms, and
//! ignores a second switch sent right after the first.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use simrig_plugin_kit::sysfs;

/// How long to wait before switching back, and between attempts.
const PAUSE: Duration = Duration::from_millis(1500);
const ATTEMPTS: u8 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unlock {
    /// Nothing asked yet.
    Idle,
    /// The menu was switched: waiting for the base to send it.
    Asked { at: Instant, tries: u8 },
    /// The menu arrived: switching it back to `mode`.
    Restoring { mode: i64, at: Instant, tries: u8 },
    /// The menu is known (or SimRig gave up asking): slots are remembered from now on.
    Done,
}

/// The tuning menu's directory of a base (`ftec_tuning/<name>/`).
pub struct Menu<'a> {
    pub dir: &'a Path,
}

impl Menu<'_> {
    fn read(&self, entry: &str) -> Option<i64> {
        sysfs::read_int(&self.dir.join(entry))
    }

    fn write(&self, entry: &str, value: i64) {
        let _ = sysfs::write(&self.dir.join(entry), &value.to_string());
    }

    fn slot(&self) -> i64 {
        self.read("SLOT").unwrap_or(0)
    }
}

impl Unlock {
    /// One step, at every poll; `remembered` is the slot to select again.
    pub fn step(self, menu: &Menu, now: Instant, remembered: Option<i64>) -> Self {
        match self {
            Unlock::Idle if menu.slot() != 0 => Unlock::Done,
            Unlock::Idle => {
                // Until the menu arrives the driver takes the mode as standard (0): 1 makes it
                // send the switch.
                menu.write("advanced_mode", 1);
                Unlock::Asked { at: now, tries: 1 }
            }
            Unlock::Asked { at, tries } => {
                if menu.slot() != 0 {
                    // Arrived, in the other mode: the one to go back to is the opposite.
                    let mode = 1 - menu.read("advanced_mode").unwrap_or(0);
                    Unlock::Restoring {
                        mode,
                        at: now,
                        tries: 0,
                    }
                } else if now.duration_since(at) < PAUSE * 2 {
                    self
                } else if tries < 2 {
                    menu.write("advanced_mode", 1);
                    Unlock::Asked {
                        at: now,
                        tries: tries + 1,
                    }
                } else {
                    // The base does not answer: the user can still open the menu on the wheel.
                    Unlock::Done
                }
            }
            Unlock::Restoring { mode, at, tries } => {
                if tries > 0 && menu.read("advanced_mode") == Some(mode) {
                    if let Some(slot) = remembered.filter(|s| *s != menu.slot()) {
                        menu.write("SLOT", slot);
                    }
                    Unlock::Done
                } else if now.duration_since(at) < PAUSE {
                    self
                } else if tries < ATTEMPTS {
                    menu.write("advanced_mode", mode);
                    Unlock::Restoring {
                        mode,
                        at: now,
                        tries: tries + 1,
                    }
                } else {
                    Unlock::Done
                }
            }
            Unlock::Done => Unlock::Done,
        }
    }
}

/// The slot each model was last on, kept next to the plugin's configuration (`<config>.slots`,
/// lines of `<pid> <slot>`).
pub struct Slots(pub PathBuf);

impl Slots {
    pub fn for_config(config: &Path) -> Self {
        Slots(config.with_extension("slots"))
    }

    pub fn get(&self, key: &str) -> Option<i64> {
        fs::read_to_string(&self.0).ok()?.lines().find_map(|l| {
            let (k, v) = l.split_once(' ')?;
            (k == key).then(|| v.trim().parse().ok())?
        })
    }

    pub fn set(&self, key: &str, slot: i64) {
        if self.get(key) == Some(slot) {
            return;
        }
        let mut lines: Vec<String> = fs::read_to_string(&self.0)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.starts_with(&format!("{key} ")))
            .map(str::to_string)
            .collect();
        lines.push(format!("{key} {slot}"));
        if let Some(dir) = self.0.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(&self.0, lines.join("\n") + "\n");
    }
}
