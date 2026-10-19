//! The device checks on screen: what may keep games from using the open device (one block
//! above its settings, ignored problems in one line), the short form on its Home card and the
//! count in the rig summary. Nothing is shown when nothing is found.

use slint::{ComponentHandle, ModelRc, VecModel};

use simrig_host::device_checks::Node;
use simrig_host::{DeviceFindings, Finding, Fix, PluginLocation, Problem};

use crate::app::App;
use crate::backend::Command;
use crate::ui::{FindingData, Rig};

#[derive(Default)]
pub struct ChecksState {
    /// The ignored problems of the open device are shown again.
    pub ignored_open: bool,
}

/// Two or three words for a Home card.
fn short(finding: &Finding) -> &'static str {
    match finding.problem {
        Problem::NoAccess { .. }
        | Problem::NotJoystick { .. }
        | Problem::WrongDriver { bound: None, .. } => "Hidden from games",
        Problem::WrongDriver { .. } => "No force feedback in games",
        Problem::OtherCopy { .. } => "Other driver copy loaded",
        Problem::SteamInput { .. } | Problem::SteamGeneric => "Steam Input on",
    }
}

impl App {
    /// The findings of the device a component belongs to.
    pub fn findings_of(&self, address: &str) -> Option<&DeviceFindings> {
        let c = &self.view(address)?.component;
        self.snapshot
            .findings
            .iter()
            .find(|f| f.plugin == c.plugin && f.device == c.device.id)
    }

    /// What a Home card says when its device needs attention: the first warning not ignored.
    /// Said once, on the device's first card (a base and its rim are one device).
    pub fn attention(&self, address: &str) -> Option<&'static str> {
        let c = &self.view(address)?.component;
        let first = self
            .snapshot
            .components
            .iter()
            .map(|v| &v.component)
            .filter(|o| o.plugin == c.plugin && o.device.id == c.device.id)
            .min_by_key(|o| {
                (
                    o.description.kind != simrig_model::ComponentKind::WheelBase,
                    &o.address,
                )
            })?;
        if first.address != c.address {
            return None;
        }
        self.findings_of(address)?
            .active()
            .find(|f| !f.informational())
            .map(short)
    }

    /// Devices with a warning not ignored.
    pub fn needing_attention(&self) -> usize {
        self.snapshot
            .findings
            .iter()
            .filter(|d| d.active().any(|f| !f.informational()))
            .count()
    }

    fn finding_data(&self, plugin: &str, finding: &Finding) -> FindingData {
        let privileged = self
            .snapshot
            .plugins
            .iter()
            .any(|p| p.id == plugin && p.location == PluginLocation::System);
        let fix = match finding.fix {
            Some(Fix::Rebind) => "Reload the driver",
            None => "",
        };
        FindingData {
            key: finding.key().into(),
            title: finding.title().into(),
            body: finding.explanation().into(),
            detail: finding.detail().into(),
            info: finding.informational(),
            fix_note: if !fix.is_empty() && privileged {
                "Asks for the password once.".into()
            } else {
                Default::default()
            },
            fix: fix.into(),
            fix_privileged: privileged,
            fixing: self.flows.first_installing.contains(plugin),
            ignorable: finding.ignorable(),
            ignored: finding.ignored,
        }
    }

    /// The block and the ignored line of the open component's device.
    pub fn refresh_findings(&mut self) {
        let address = self.current.clone().unwrap_or_default();
        let (mut active, mut ignored) = (Vec::new(), Vec::new());
        if let Some(device) = self.findings_of(&address) {
            for f in &device.findings {
                let data = self.finding_data(&device.plugin, f);
                if f.ignored {
                    ignored.push(data);
                } else {
                    active.push(data);
                }
            }
        }
        // Warnings first: the block's border is a warning's when its first finding is one.
        active.sort_by_key(|f| f.info);
        ignored.sort_by_key(|f| f.info);
        if ignored.is_empty() {
            self.checks.ignored_open = false;
        }
        let window = self.window();
        let rig = window.global::<Rig>();
        rig.set_ignored_summary(
            match ignored.len() {
                1 => "1 problem ignored".to_string(),
                n => format!("{n} problems ignored"),
            }
            .into(),
        );
        rig.set_current_findings(ModelRc::new(VecModel::from(active)));
        rig.set_current_ignored(ModelRc::new(VecModel::from(ignored)));
        rig.set_ignored_open(self.checks.ignored_open);
    }

    /// New results of the checks, from the host thread.
    pub fn findings_changed(&mut self, findings: Vec<DeviceFindings>) {
        self.snapshot.findings = findings;
        self.refresh_findings();
        self.update_cards(|_| true);
        self.refresh_rig_status();
    }

    /// Runs the fix of a finding of the open device: installing its plugin's driver again.
    pub fn fix_finding(&mut self, key: &str) {
        let Some(device) = self.findings_of(&self.current.clone().unwrap_or_default()) else {
            return;
        };
        let fixable = device
            .findings
            .iter()
            .any(|f| f.key() == key && f.fix.is_some());
        let plugin = device.plugin.clone();
        if fixable {
            self.install_driver(&plugin);
            self.refresh_findings();
        }
    }

    pub fn ignore_finding(&mut self, key: &str, ignored: bool) {
        let Some(device) = self.findings_of(&self.current.clone().unwrap_or_default()) else {
            return;
        };
        let device = device.device.clone();
        self.send(Command::SetIgnored {
            device,
            key: key.into(),
            ignored,
        });
    }

    pub fn open_ignored(&mut self, open: bool) {
        self.checks.ignored_open = open;
        self.window().global::<Rig>().set_ignored_open(open);
    }

    /// Made-up findings on the open component's device, for screenshots: a comma-separated list
    /// of `access`, `joystick`, `driver`, `copy`, `steam`, `generic`; `!` before one marks it
    /// ignored, e.g. `findings:driver,!steam`.
    pub fn preview_findings(&mut self, list: &str) {
        let Some(c) = self
            .current
            .clone()
            .and_then(|a| self.view(&a).map(|v| v.component.clone()))
        else {
            return;
        };
        let findings = list
            .split(',')
            .filter_map(|item| {
                let (ignored, name) = match item.strip_prefix('!') {
                    Some(name) => (true, name),
                    None => (false, item),
                };
                let (problem, fix) = match name {
                    "access" => (
                        Problem::NoAccess {
                            nodes: vec![Node {
                                path: "/dev/input/event7".into(),
                                mode: "crw-rw---- root:input".into(),
                            }],
                        },
                        None,
                    ),
                    "joystick" => (
                        Problem::NotJoystick {
                            node: "event7".into(),
                            props: vec!["ID_INPUT_KEY=1".into()],
                        },
                        None,
                    ),
                    "driver" => (
                        Problem::WrongDriver {
                            bound: Some("hid-generic".into()),
                            expected: "fanatec".into(),
                        },
                        Some(Fix::Rebind),
                    ),
                    "copy" => (
                        Problem::OtherCopy {
                            module: "hid_fanatec".into(),
                            loaded: "0.9 (5C3E1A0F)".into(),
                            installed: "0.11 (9B2D44E1)".into(),
                            file: "/lib/modules/7.0.0-38-generic/updates/dkms/hid-fanatec.ko.zst"
                                .into(),
                        },
                        None,
                    ),
                    "steam" => (
                        Problem::SteamInput {
                            app: "805550".into(),
                            game: "Assetto Corsa Competizione".into(),
                        },
                        None,
                    ),
                    "generic" => (Problem::SteamGeneric, None),
                    _ => return None,
                };
                Some(Finding {
                    problem,
                    fix,
                    ignored,
                })
            })
            .collect();
        self.findings_changed(vec![DeviceFindings {
            plugin: c.plugin,
            device: c.device.id,
            findings,
        }]);
    }
}
