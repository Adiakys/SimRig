//! The modes of lg4ff wheels (`alternate_modes`, in the kernel's driver and new-lg4ff): a wheel
//! can present itself as an older model, for games that do not know it. Switching makes the
//! wheel reconnect with the other model's product id.

use simrig_plugin_kit::device::HidDevice;
use simrig_plugin_kit::sysfs;
use simrig_plugin_sdk::prelude::*;

pub const ATTR: &str = "alternate_modes";
pub const ID: &str = "mode";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mode {
    /// What the driver takes to switch to it: `DF-EX`, `DFP`, `G25`…
    pub tag: String,
    pub name: String,
    pub selected: bool,
}

/// The driver's list: `TAG: Name` lines, ` *` after the mode in use. The `native` line is left
/// out: the wheel's own model is in the list too, under its tag.
pub fn parse(text: &str) -> Vec<Mode> {
    text.lines()
        .filter_map(|line| {
            let (tag, name) = line.split_once(": ")?;
            let (name, selected) = match name.trim_end().strip_suffix(" *") {
                Some(name) => (name, true),
                None => (name.trim_end(), false),
            };
            (tag != "native").then(|| Mode {
                tag: tag.into(),
                name: name.into(),
                selected,
            })
        })
        .collect()
}

/// The modes as a custom parameter, when the driver offers more than one.
pub fn param(hid: &HidDevice) -> Option<CustomParam> {
    let modes = parse(&sysfs::read(&hid.attr(ATTR)?)?);
    if modes.len() < 2 {
        return None;
    }
    let options: Vec<(&str, &str)> = modes
        .iter()
        .map(|m| (m.tag.as_str(), m.name.as_str()))
        .collect();
    let current = modes
        .iter()
        .find(|m| m.selected)
        .unwrap_or(&modes[modes.len() - 1]);
    let spec = ParamSpec::choice(&current.tag, &options);
    Some(
        CustomParam::new(ID, "Model presented to games", spec.ty, spec.value)
            .help(
                "Older games that do not know the wheel can use it as an older model. The wheel \
                 reconnects as that model, which SimRig then shows, until it is plugged in again.",
            )
            .group("Compatibility")
            .advanced(),
    )
}

pub fn set(hid: &HidDevice, value: &Value) -> Result<()> {
    let Value::Choice(tag) = value else {
        return Err(Error::InvalidValue("expected a mode".into()));
    };
    hid.write(ATTR, tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drivers_list() {
        let text = "native: G29 Driving Force Racing Wheel *\nDF-EX: Driving Force / Formula EX\n\
                    DFP: Driving Force Pro\nG25: G25 Racing Wheel\nDFGT: Driving Force GT\n\
                    G27: G27 Racing Wheel\nG29: G29 Racing Wheel *\n";
        let modes = parse(text);
        assert_eq!(modes.len(), 6);
        assert_eq!(modes[0].tag, "DF-EX");
        assert_eq!(modes[0].name, "Driving Force / Formula EX");
        assert!(!modes[0].selected);
        assert!(modes[5].selected);
        assert_eq!(modes[5].name, "G29 Racing Wheel");
    }
}
