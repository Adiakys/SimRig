//! `plugin.toml`: what a plugin is, which devices it handles and how to start its adapter.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::HostError;
use crate::usb::UsbDevice;

pub const MANIFEST_FILE: &str = "plugin.toml";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Adapter protocol version.
    pub api: u32,
    /// Adapter executable, relative to the plugin directory.
    pub exec: String,
    /// Arguments passed to the adapter.
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "match")]
    pub matching: Match,
    /// Path of the driver contract (`driver.toml`), when the plugin ships a driver.
    #[serde(default)]
    pub driver: Option<String>,
    /// Catalog models confirmed on real hardware (`fanatec/csl-dd`), or `*` for every device
    /// the plugin handles. The app marks the others as untested, and invites a report.
    #[serde(default)]
    pub tested: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Match {
    #[serde(default)]
    pub usb: Vec<UsbMatch>,
    /// The plugin provides devices by itself (e.g. the simulated rig).
    #[serde(default, rename = "virtual")]
    pub virtual_devices: bool,
    /// The plugin handles any USB device the user assigns to it explicitly (generic plugins).
    /// It never gets devices on its own.
    #[serde(default)]
    pub on_request: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsbMatch {
    pub vid: u16,
    /// Any product of the vendor when absent.
    #[serde(default)]
    pub pid: Option<u16>,
}

impl UsbMatch {
    pub fn matches(&self, vid: u16, pid: u16) -> bool {
        self.vid == vid && self.pid.is_none_or(|p| p == pid)
    }

    /// How specific the match is: 2 for vendor and product, 1 for vendor only.
    fn specificity(&self) -> u8 {
        if self.pid.is_some() { 2 } else { 1 }
    }
}

impl Match {
    /// Best specificity with which this plugin claims a USB device, if at all.
    pub fn usb_claim(&self, vid: u16, pid: u16) -> Option<u8> {
        self.usb
            .iter()
            .filter(|m| m.matches(vid, pid))
            .map(UsbMatch::specificity)
            .max()
    }
}

/// Gives each USB device to exactly one plugin: the most specific match wins (vendor and
/// product over vendor only); on a tie, the plugin found first (explicit, then user, then
/// system directories). Plugins of different brands never compete, so any mix of brands works.
/// Returns the devices of each plugin, in the order of `manifests`, and the ties found.
///
/// `overrides` maps a device key ([`UsbDevice::keys`]) to the plugin the user chose for it; the
/// choice wins as long as that plugin claims the device at all.
pub fn assign_usb(
    manifests: &[&Manifest],
    devices: &[UsbDevice],
    overrides: &std::collections::BTreeMap<String, String>,
) -> (Vec<Vec<UsbDevice>>, Vec<String>) {
    let mut assigned = vec![Vec::new(); manifests.len()];
    let mut problems = Vec::new();
    for device in devices {
        let claims: Vec<(usize, u8)> = manifests
            .iter()
            .enumerate()
            .filter_map(|(i, m)| m.matching.usb_claim(device.vid, device.pid).map(|s| (i, s)))
            .collect();
        let chosen = device.keys().iter().find_map(|k| overrides.get(k).cloned());
        // The user's choice wins if that plugin claims the device or takes devices on request.
        let chosen = chosen.and_then(|p| {
            manifests
                .iter()
                .position(|m| m.id == p)
                .filter(|i| manifests[*i].matching.on_request || claims.iter().any(|(c, _)| c == i))
        });
        if let Some(i) = chosen {
            assigned[i].push(device.clone());
            continue;
        }
        let Some(best) = claims.iter().map(|(_, s)| *s).max() else {
            continue;
        };
        let winners: Vec<usize> = claims
            .iter()
            .filter(|(_, s)| *s == best)
            .map(|(i, _)| *i)
            .collect();
        if winners.len() > 1 {
            let ids: Vec<&str> = winners.iter().map(|i| manifests[*i].id.as_str()).collect();
            problems.push(format!(
                "{} ({:04x}:{:04x}) is claimed by plugins {}; using '{}'",
                device.name,
                device.vid,
                device.pid,
                ids.join(", "),
                ids[0]
            ));
        }
        assigned[winners[0]].push(device.clone());
    }
    (assigned, problems)
}

/// Where a plugin was found. Only system plugins may install drivers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginLocation {
    System,
    User,
    /// A directory given explicitly (development).
    Explicit,
}

impl Manifest {
    /// A device of this catalog model has not been confirmed on real hardware. Devices a
    /// plugin provides by itself (a simulated rig) are not real hardware to confirm.
    pub fn untested(&self, model: Option<&str>) -> bool {
        if self.matching.virtual_devices {
            return false;
        }
        !self
            .tested
            .iter()
            .any(|t| t == "*" || Some(t.as_str()) == model)
    }

    pub fn load(path: &Path) -> Result<Self, HostError> {
        let invalid = |message: String| HostError::Invalid {
            path: path.to_path_buf(),
            message,
        };
        let text = fs::read_to_string(path).map_err(|e| invalid(e.to_string()))?;
        let manifest: Manifest = toml::from_str(&text).map_err(|e| invalid(e.to_string()))?;
        let valid_id = !manifest.id.is_empty()
            && manifest
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
        if !valid_id {
            return Err(invalid(format!(
                "id '{}' must use a-z, 0-9, '-' or '_'",
                manifest.id
            )));
        }
        if manifest.exec.contains('/') || manifest.exec.contains("..") {
            return Err(invalid(
                "exec must be a file name inside the plugin directory".into(),
            ));
        }
        Ok(manifest)
    }
}

/// A plugin found on disk.
#[derive(Clone, Debug)]
pub struct Found {
    pub manifest: Manifest,
    pub dir: PathBuf,
    pub location: PluginLocation,
}

/// Default plugin directories, user first, then system.
pub fn default_dirs() -> Vec<(PathBuf, PluginLocation)> {
    let mut dirs = Vec::new();
    if let Some(data) = crate::paths::data_dir() {
        dirs.push((data.join("plugins"), PluginLocation::User));
    }
    dirs.push((
        PathBuf::from("/usr/local/lib/simrig/plugins"),
        PluginLocation::System,
    ));
    dirs.push((
        PathBuf::from("/usr/lib/simrig/plugins"),
        PluginLocation::System,
    ));
    dirs
}

/// Finds plugins in `dirs`. A directory may be a plugin itself or contain plugins. When two
/// plugins share an id, the first one wins.
pub fn discover(dirs: &[(PathBuf, PluginLocation)]) -> (Vec<Found>, Vec<String>) {
    let mut found: Vec<Found> = Vec::new();
    let mut problems = Vec::new();
    for (dir, location) in dirs {
        let candidates: Vec<PathBuf> = if dir.join(MANIFEST_FILE).is_file() {
            vec![dir.clone()]
        } else {
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            let mut subdirs: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.join(MANIFEST_FILE).is_file())
                .collect();
            subdirs.sort();
            subdirs
        };
        for plugin_dir in candidates {
            match Manifest::load(&plugin_dir.join(MANIFEST_FILE)) {
                Ok(manifest) if found.iter().any(|f| f.manifest.id == manifest.id) => {
                    problems.push(format!(
                        "plugin '{}' in {} ignored: already found elsewhere",
                        manifest.id,
                        plugin_dir.display()
                    ));
                }
                Ok(manifest) => found.push(Found {
                    manifest,
                    dir: plugin_dir,
                    location: *location,
                }),
                Err(e) => problems.push(e.to_string()),
            }
        }
    }
    (found, problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_manifest() {
        let m: Manifest = toml::from_str(
            r#"
            id = "fanatec"
            name = "Fanatec"
            version = "0.3.0"
            api = 1
            exec = "simrig-plugin-fanatec"
            driver = "driver/driver.toml"
            [match]
            usb = [{ vid = 0x0eb7 }, { vid = 0x1234, pid = 0x0001 }]
            "#,
        )
        .unwrap();
        assert!(m.matching.usb[0].matches(0x0eb7, 0x0020));
        assert!(!m.matching.usb[1].matches(0x1234, 0x0002));
        assert!(!m.matching.virtual_devices);
    }

    fn manifest(id: &str, usb: Vec<UsbMatch>) -> Manifest {
        Manifest {
            id: id.into(),
            name: id.into(),
            version: "1".into(),
            api: 1,
            exec: id.into(),
            args: Vec::new(),
            description: None,
            matching: Match {
                usb,
                virtual_devices: false,
                on_request: false,
            },
            driver: None,
            tested: Vec::new(),
        }
    }

    #[test]
    fn untested_models() {
        let mut m = manifest("fanatec", vec![]);
        assert!(m.untested(Some("fanatec/csl-dd")));
        m.tested = vec!["fanatec/csl-dd".into()];
        assert!(!m.untested(Some("fanatec/csl-dd")));
        assert!(m.untested(Some("fanatec/csl-elite")));
        assert!(m.untested(None));
        m.tested = vec!["*".into()];
        assert!(!m.untested(None));
        // A simulated rig is not hardware to confirm.
        let mut sim = manifest("sim", vec![]);
        sim.matching.virtual_devices = true;
        assert!(!sim.untested(None));
    }

    fn device(vid: u16, pid: u16) -> UsbDevice {
        UsbDevice {
            vid,
            pid,
            sysfs: PathBuf::from(format!("/sys/bus/usb/devices/{vid:04x}-{pid:04x}")),
            name: format!("{vid:04x}:{pid:04x}"),
            serial: None,
            id: format!("usb-{vid:04x}-{pid:04x}"),
        }
    }

    #[test]
    fn mixed_brands_each_get_their_devices() {
        let fanatec = manifest(
            "fanatec",
            vec![UsbMatch {
                vid: 0x0eb7,
                pid: None,
            }],
        );
        let moza = manifest(
            "moza",
            vec![UsbMatch {
                vid: 0x346e,
                pid: None,
            }],
        );
        let generic = manifest(
            "generic-hid",
            vec![UsbMatch {
                vid: 0x346e,
                pid: Some(0x1000),
            }],
        );
        let devices = [
            device(0x0eb7, 0x0020),
            device(0x346e, 0x0003),
            device(0x346e, 0x1000),
            device(0x1234, 1),
        ];
        let (assigned, problems) =
            assign_usb(&[&fanatec, &moza, &generic], &devices, &Default::default());
        assert!(problems.is_empty());
        assert_eq!(assigned[0], vec![devices[0].clone()]);
        assert_eq!(assigned[1], vec![devices[1].clone()]);
        // the product-specific match wins over the vendor-wide one
        assert_eq!(assigned[2], vec![devices[2].clone()]);
    }

    #[test]
    fn ties_go_to_the_first_plugin_and_are_reported() {
        let a = manifest(
            "a",
            vec![UsbMatch {
                vid: 0x0eb7,
                pid: None,
            }],
        );
        let b = manifest(
            "b",
            vec![UsbMatch {
                vid: 0x0eb7,
                pid: None,
            }],
        );
        let (assigned, problems) =
            assign_usb(&[&a, &b], &[device(0x0eb7, 0x0020)], &Default::default());
        assert_eq!((assigned[0].len(), assigned[1].len()), (1, 0));
        assert_eq!(problems.len(), 1);
        // the user's choice wins, and removes the ambiguity
        let overrides = [("usb:0eb7:0020".to_string(), "b".to_string())].into();
        let (assigned, problems) = assign_usb(&[&a, &b], &[device(0x0eb7, 0x0020)], &overrides);
        assert_eq!((assigned[0].len(), assigned[1].len()), (0, 1));
        assert!(problems.is_empty());
    }

    #[test]
    fn generic_plugins_get_only_devices_assigned_to_them() {
        let mut generic = manifest("generic-usb", vec![]);
        generic.matching.on_request = true;
        let unknown = device(0x1234, 0x0001);
        let (assigned, _) = assign_usb(
            &[&generic],
            std::slice::from_ref(&unknown),
            &Default::default(),
        );
        assert!(assigned[0].is_empty());
        let overrides = [("usb:1234:0001".to_string(), "generic-usb".to_string())].into();
        let (assigned, _) = assign_usb(&[&generic], &[unknown], &overrides);
        assert_eq!(assigned[0].len(), 1);
    }

    #[test]
    fn rejects_unknown_fields() {
        let r: Result<Manifest, _> =
            toml::from_str("id='a'\nname='A'\nversion='1'\napi=1\nexec='a'\nbogus=1");
        assert!(r.is_err());
    }
}
