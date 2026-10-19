//! A hardware report: the project's GitHub form, filled in with what SimRig knows about a
//! device (model, USB id, driver, system, what it sees as supported). The user reads it, says
//! how it went and sends it from the browser: SimRig sends nothing.

use simrig_host::SystemInfo;
use simrig_model::{Bus, ItemClass, Support};

use crate::backend::{ComponentView, PluginView};
use crate::update::Format;

const FORM: &str = "https://github.com/Adiakys/SimRig/issues/new?template=hardware-report.yml";

/// What the report says, before it becomes a link.
pub struct Report<'a> {
    pub view: &'a ComponentView,
    pub plugin: Option<&'a PluginView>,
    pub system: Option<&'a SystemInfo>,
    pub version: &'a str,
    pub format: Option<Format>,
}

impl Report<'_> {
    pub fn url(&self) -> String {
        let c = &self.view.component;
        let d = &c.description;
        let brand = &self.view.plugin_name;
        let mut fields = vec![
            ("title", format!("[Hardware] {brand} {}", d.name)),
            // Prefilled only when it is one of the form's options.
            ("brand", brand.clone()),
            ("model", d.name.clone()),
            ("version", self.version.into()),
            ("install", install(self.format).into()),
            ("driver", driver(self.plugin).into()),
            ("details", self.details()),
        ];
        if let Bus::Usb { vid, pid, .. } = &c.device.bus {
            fields.push(("usb", format!("{vid:04x}:{pid:04x} {}", c.device.name)));
        }
        if let Some(system) = self.system {
            fields.push(("system", system_line(system)));
        }
        let query: String = fields
            .iter()
            .map(|(k, v)| format!("&{k}={}", encode(v)))
            .collect();
        format!("{FORM}{query}")
    }

    /// For the maintainers: the plugin, the driver, and what SimRig sees on the device.
    fn details(&self) -> String {
        let c = &self.view.component;
        let d = &c.description;
        let mut lines = vec![format!(
            "From SimRig: {} ({}), component `{}`{}.",
            d.kind.label(),
            self.view.plugin_name,
            c.address,
            d.model
                .as_deref()
                .map(|m| format!(", model `{m}`"))
                .unwrap_or_default()
        )];
        if let Some(p) = self.plugin {
            let driver = match (&p.driver_name, &p.driver) {
                (Some(name), Some(s)) => format!(
                    ", driver {name} {} ({})",
                    s.version.as_deref().unwrap_or("?"),
                    if s.provided_by_system {
                        "provided by the system"
                    } else if s.installed && s.active {
                        "active"
                    } else if s.installed {
                        "installed"
                    } else {
                        "not installed"
                    }
                ),
                _ => String::new(),
            };
            lines.push(format!("Plugin {} {}{driver}.", p.id, p.version));
        }
        let (mut supported, mut unsupported) = (Vec::new(), Vec::new());
        for item in d.items.iter().filter(|i| i.class != ItemClass::Output) {
            match &item.support {
                Support::Supported(_) => supported.push(item.id.as_str()),
                Support::Unsupported(u) => {
                    unsupported.push(format!("{} ({:?})", item.id, u.reason))
                }
            }
        }
        lines.push(format!("Supported: {}.", supported.join(", ")));
        if !unsupported.is_empty() {
            lines.push(format!("Not supported: {}.", unsupported.join(", ")));
        }
        lines.join("\n")
    }
}

/// The form's options for how SimRig is installed.
fn install(format: Option<Format>) -> &'static str {
    match format {
        Some(Format::Deb) => ".deb package",
        Some(Format::Rpm) => ".rpm package",
        Some(Format::Arch) => "Arch package",
        Some(Format::Tarball) => ".tar.gz",
        None => "Built from the sources",
    }
}

/// The form's options for the device's driver.
fn driver(plugin: Option<&PluginView>) -> &'static str {
    match plugin {
        Some(p) if !p.has_driver => "None needed (it works with the kernel's driver)",
        Some(p) => match &p.driver {
            Some(s) if s.provided_by_system => {
                "I had already installed it myself (package, AUR, by hand)"
            }
            Some(s) if s.installed => "SimRig installed it",
            _ => "It did not get installed (see below)",
        },
        None => "I don't know",
    }
}

fn system_line(s: &SystemInfo) -> String {
    let mut parts = Vec::new();
    if let Some(distro) = &s.distro {
        parts.push(distro.clone());
    }
    if let Some(kernel) = &s.kernel {
        parts.push(format!("kernel {kernel}"));
    }
    if let Some(sb) = s.secure_boot {
        parts.push(format!("Secure Boot {}", if sb { "on" } else { "off" }));
    }
    parts.join(", ")
}

/// Percent-encoding for a query value: everything but unreserved characters.
fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl crate::app::App {
    /// Opens the hardware report of the open component in the browser.
    pub fn report_device(&mut self) {
        let Some(address) = self.current.clone() else {
            return;
        };
        let Some(view) = self.view(&address) else {
            return;
        };
        let plugin = self
            .snapshot
            .plugins
            .iter()
            .find(|p| p.id == view.component.plugin);
        let url = Report {
            view,
            plugin,
            system: self.snapshot.system.as_ref(),
            version: env!("CARGO_PKG_VERSION"),
            format: Format::installed(),
        }
        .url();
        crate::update::open_url(&url);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_values_are_encoded() {
        assert_eq!(encode("CSL DD, kernel 7.0"), "CSL%20DD%2C%20kernel%207.0");
        assert_eq!(encode("Ü"), "%C3%9C");
    }

    /// The options filled in are the form's own.
    #[test]
    fn form_options_exist() {
        let form = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../.github/ISSUE_TEMPLATE/hardware-report.yml"
        ))
        .unwrap();
        let formats = [
            None,
            Some(Format::Deb),
            Some(Format::Rpm),
            Some(Format::Arch),
            Some(Format::Tarball),
        ];
        for option in formats.map(install).into_iter().chain([driver(None)]) {
            assert!(form.contains(&format!("- {option}")), "{option}");
        }
        for option in [
            "None needed (it works with the kernel's driver)",
            "I had already installed it myself (package, AUR, by hand)",
            "SimRig installed it",
            "It did not get installed (see below)",
        ] {
            assert!(form.contains(&format!("- {option}")), "{option}");
        }
        for id in [
            "brand", "model", "usb", "system", "version", "install", "driver", "details",
        ] {
            assert!(form.contains(&format!("id: {id}")), "{id}");
        }
    }

    #[test]
    fn a_filled_in_report() {
        use simrig_host::{HostComponent, Image};
        use simrig_model::{ComponentDescription, ComponentKind, DeviceInfo};
        let view = ComponentView {
            component: HostComponent {
                address: "fanatec/usb-0eb7-0020/base".into(),
                plugin: "fanatec".into(),
                device: DeviceInfo {
                    id: "usb-0eb7-0020".into(),
                    name: "FANATEC Wheel".into(),
                    bus: Bus::Usb {
                        vid: 0x0eb7,
                        pid: 0x0020,
                        sysfs: String::new(),
                        serial: None,
                    },
                },
                description: ComponentDescription {
                    id: "base".into(),
                    kind: ComponentKind::WheelBase,
                    name: "CSL DD".into(),
                    model: Some("fanatec/csl-dd".into()),
                    pedals: vec![],
                    items: vec![],
                    custom: vec![],
                    procedures: vec![],
                    appearance: Default::default(),
                    controls: vec![],
                },
                excluded: false,
            },
            image: Image::Generic {
                kind: ComponentKind::WheelBase,
                rim: None,
            },
            plugin_name: "Fanatec".into(),
            system_plugin: true,
            untested: false,
        };
        let url = Report {
            view: &view,
            plugin: None,
            system: None,
            version: "0.1.0",
            format: Some(Format::Deb),
        }
        .url();
        assert!(url.starts_with(FORM), "{url}");
        assert!(url.contains("&brand=Fanatec&"), "{url}");
        assert!(url.contains("&usb=0eb7%3A0020%20FANATEC%20Wheel"), "{url}");
        assert!(url.contains("&install=.deb%20package&"), "{url}");
        assert!(url.contains("model%20%60fanatec%2Fcsl-dd%60"), "{url}");
    }

    #[test]
    fn form_options() {
        assert_eq!(install(None), "Built from the sources");
        assert_eq!(install(Some(Format::Deb)), ".deb package");
        assert_eq!(driver(None), "I don't know");
    }
}
