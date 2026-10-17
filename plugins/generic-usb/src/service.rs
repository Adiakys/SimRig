//! The driver's service: started by udev when a set-up device is plugged in, it reads the
//! device's raw reports (from `hidraw`, or straight from USB for devices the kernel's HID
//! driver rejects) and drives a virtual joystick every game understands. It runs as long as the
//! device is connected, with or without the app.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use evdev::uinput::VirtualDevice;
use evdev::{
    AbsInfo, AbsoluteAxisCode, AttributeSet, BusType, EventType, InputEvent, InputId, KeyCode,
    UinputAbsSetup,
};

use crate::adapter::joystick_name;
use crate::config::{DeviceConfig, Setup};
use crate::decode::State;
use crate::hid;

/// Full scale of the handbrake axis.
///
/// The axis is `ABS_RX`: udev counts it as a joystick axis (it does not count `ABS_X` alone),
/// so a one-button handbrake is still classified as a joystick and games can open it.
pub const AXIS_MAX: i32 = 65_535;
const AXIS: AbsoluteAxisCode = AbsoluteAxisCode::ABS_RX;

/// What the virtual joystick shows for a state: its buttons, and its axis for handbrakes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Output {
    pub buttons: Vec<bool>,
    pub axis: Option<i32>,
}

/// Buttons: one per gear and reverse (H-pattern), up and down (sequential), one for the
/// handbrake (pulled, in button mode or for switch handbrakes). Handbrakes also have an axis.
pub fn output(config: &DeviceConfig, state: &State) -> Output {
    match &config.setup {
        Setup::HShifter { gears, .. } => Output {
            buttons: (0..=usize::from(*gears))
                .map(|i| state.slot == Some(i))
                .collect(),
            axis: None,
        },
        Setup::Sequential { .. } => Output {
            buttons: state.sequential.to_vec(),
            axis: None,
        },
        Setup::Handbrake(_) => Output {
            buttons: vec![state.pressed],
            axis: Some((state.position * f64::from(AXIS_MAX)).round() as i32),
        },
    }
}

fn button(i: usize) -> KeyCode {
    // BTN_TRIGGER and the joystick buttons after it, then the extra ones.
    if i < 16 {
        KeyCode::new(KeyCode::BTN_TRIGGER.code() + i as u16)
    } else {
        KeyCode::new(KeyCode::BTN_TRIGGER_HAPPY1.code() + (i - 16) as u16)
    }
}

fn create(config: &DeviceConfig) -> std::io::Result<VirtualDevice> {
    let shape = output(config, &State::default());
    let mut keys = AttributeSet::<KeyCode>::new();
    for i in 0..shape.buttons.len().max(1) {
        keys.insert(button(i));
    }
    let name = joystick_name(config);
    let mut builder = VirtualDevice::builder()?
        .name(&name)
        .input_id(InputId::new(
            BusType::BUS_VIRTUAL,
            config.vid,
            config.pid,
            1,
        ))
        .with_keys(&keys)?;
    if shape.axis.is_some() {
        let axis = UinputAbsSetup::new(AXIS, AbsInfo::new(0, 0, AXIS_MAX, 0, 0, 0));
        builder = builder.with_absolute_axis(&axis)?;
    }
    builder.build()
}

fn emit(device: &mut VirtualDevice, old: &Output, new: &Output) -> std::io::Result<()> {
    let mut events = Vec::new();
    for (i, pressed) in new.buttons.iter().enumerate() {
        if old.buttons.get(i) != Some(pressed) {
            events.push(InputEvent::new(
                EventType::KEY.0,
                button(i).code(),
                i32::from(*pressed),
            ));
        }
    }
    if let Some(axis) = new.axis.filter(|a| old.axis != Some(*a)) {
        events.push(InputEvent::new(EventType::ABSOLUTE.0, AXIS.0, axis));
    }
    if events.is_empty() {
        return Ok(());
    }
    device.emit(&events)
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Serves `/dev/<hidraw>` until the device goes away. Returns at once when the device (or this
/// interface of it) is not set up.
pub fn run(hidraw: &str, config_dir: &Path) -> Result<(), String> {
    let (usb, vid, pid, serial) =
        hid::usb_device_of(hidraw).ok_or_else(|| format!("{hidraw} is not a USB HID device"))?;
    let node = PathBuf::from("/dev").join(hidraw);
    let interface = hid::hidraw_nodes(&usb)
        .iter()
        .position(|n| *n == node)
        .unwrap_or(0);
    let Some(config) = DeviceConfig::load(config_dir, vid, pid, serial.as_deref()) else {
        eprintln!("{vid:04x}:{pid:04x} is not set up: nothing to do");
        return Ok(());
    };
    if config.interface != interface {
        // Another interface of the device carries the inputs, served by its own instance.
        return Ok(());
    }
    let mut file = File::open(&node).map_err(|e| format!("cannot open {}: {e}", node.display()))?;
    eprintln!("{}: serving {hidraw}", joystick_name(&config));
    serve(config, config_dir, None, |buf| file.read(buf))
}

/// Serves the USB device `name` (as in `/sys/bus/usb/devices`) straight from USB: for devices
/// the kernel's HID driver rejects, which have no `hidraw`. Returns at once when the device has
/// `hidraw` (served by its own instance) or is not set up. Relays the reports to the app.
pub fn run_usb(name: &str, config_dir: &Path) -> Result<(), String> {
    let usb = Path::new("/sys/bus/usb/devices").join(name);
    let read = |f: &str| {
        fs::read_to_string(usb.join(f))
            .ok()
            .map(|s| s.trim().to_string())
    };
    let (Some(vid), Some(pid)) = (
        read("idVendor").and_then(|v| u16::from_str_radix(&v, 16).ok()),
        read("idProduct").and_then(|p| u16::from_str_radix(&p, 16).ok()),
    ) else {
        return Err(format!("{name} is not a USB device"));
    };
    if !hid::hidraw_nodes(&usb).is_empty() {
        return Ok(());
    }
    let serial = read("serial").filter(|s| !s.is_empty());
    let Some(config) = DeviceConfig::load(config_dir, vid, pid, serial.as_deref()) else {
        eprintln!("{vid:04x}:{pid:04x} is not set up: nothing to do");
        return Ok(());
    };
    // The app may still hold the device, while it saves the setup: wait for it.
    let mut interfaces = loop {
        match crate::usbraw::open(&usb) {
            Ok(interfaces) => break interfaces,
            Err(_) if usb.is_dir() => std::thread::sleep(Duration::from_millis(500)),
            Err(e) => return Err(format!("cannot read {name}: {e}")),
        }
    };
    if config.interface >= interfaces.len() {
        return Err(format!("{name} has no HID interface {}", config.interface));
    }
    let mut interface = interfaces.swap_remove(config.interface);
    drop(interfaces);
    let relay = crate::usbraw::relay_path(&config.file_stem());
    eprintln!(
        "{}: serving {name} straight from USB",
        joystick_name(&config)
    );
    let result = serve(config, config_dir, Some(&relay), |buf| interface.read(buf));
    let _ = fs::remove_file(&relay);
    result
}

/// Drives the virtual joystick from the reports `read` gives, until the device goes away
/// (`read` returns 0 or fails). A new setup saved from the app is taken at once. With `relay`,
/// the reports are also written there for the app.
fn serve(
    mut config: DeviceConfig,
    config_dir: &Path,
    relay: Option<&Path>,
    mut read: impl FnMut(&mut [u8]) -> std::io::Result<usize>,
) -> Result<(), String> {
    let (vid, pid, serial) = (config.vid, config.pid, config.serial.clone());
    let config_path = config_dir.join(format!("{}.toml", config.file_stem()));
    let mut stamp = modified(&config_path);
    let mut device = create(&config).map_err(|e| format!("cannot create the joystick: {e}"))?;
    let mut buf = [0u8; 64];
    let mut last = Output::default();
    let mut last_report: Vec<u8> = Vec::new();
    loop {
        let n = match read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            // Unplugged: the unit stops with the device anyway.
            Err(e) => {
                eprintln!("read: {e}");
                return Ok(());
            }
        };
        // A new setup saved from the app: the same joystick if its shape is the same.
        let now = modified(&config_path);
        if now != stamp {
            stamp = now;
            if let Some(new) = DeviceConfig::load(config_dir, vid, pid, serial.as_deref()) {
                let reshaped = output(&new, &State::default())
                    != output(&config, &State::default())
                    || joystick_name(&new) != joystick_name(&config);
                config = new;
                if reshaped {
                    device =
                        create(&config).map_err(|e| format!("cannot create the joystick: {e}"))?;
                    last = Output::default();
                }
            }
        }
        if let Some(relay) = relay.filter(|_| buf[..n] != last_report[..]) {
            if let Err(e) = crate::usbraw::write_relay(relay, config.interface, &buf[..n]) {
                eprintln!("cannot relay the reports: {e}");
            }
            last_report = buf[..n].to_vec();
        }
        let next = output(&config, &config.decode(&buf[..n]));
        if let Err(e) = emit(&mut device, &last, &next) {
            eprintln!("cannot write to the joystick: {e}");
        }
        last = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ByteMatch, Signature};

    #[test]
    fn shifter_buttons() {
        let sig = |b: u8| Signature {
            bytes: vec![ByteMatch {
                index: 0,
                mask: b,
                value: b,
            }],
        };
        let config = DeviceConfig {
            name: "x".into(),
            vid: 1,
            pid: 2,
            serial: None,
            interface: 0,
            setup: Setup::HShifter {
                gears: 2,
                reverse_position: "right_down".into(),
                forward: vec![sig(1), sig(2)],
                reverse: sig(4),
            },
        };
        let out = output(&config, &config.decode(&[4]));
        assert_eq!(out.buttons, vec![false, false, true]);
        assert_eq!(out.axis, None);
        assert_eq!(
            output(&config, &config.decode(&[0])).buttons,
            vec![false; 3]
        );
        assert_eq!(button(0), KeyCode::BTN_TRIGGER);
        assert_eq!(button(16), KeyCode::BTN_TRIGGER_HAPPY1);
    }

    fn classified_as_joystick(config: &DeviceConfig) -> Result<(), String> {
        let mut device = create(config).map_err(|e| e.to_string())?;
        let node = device
            .enumerate_dev_nodes_blocking()
            .unwrap()
            .flatten()
            .find(|p| p.to_string_lossy().contains("event"))
            .ok_or("no event node")?;
        emit(
            &mut device,
            &Output::default(),
            &output(config, &config.decode(&[128])),
        )
        .unwrap();
        // udev classifies it as a joystick (games and SDL rely on this), once it processed it.
        let mut props = String::new();
        for _ in 0..50 {
            let info = std::process::Command::new("udevadm")
                .args(["info", "--query=property", "--name"])
                .arg(&node)
                .output()
                .unwrap();
            props = String::from_utf8_lossy(&info.stdout).into_owned();
            if props.contains("ID_INPUT=1") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if props.contains("ID_INPUT_JOYSTICK=1") {
            Ok(())
        } else {
            Err(props)
        }
    }

    /// Needs write access to /dev/uinput: `cargo test -p simrig-plugin-generic-usb -- --ignored`.
    #[test]
    #[ignore]
    fn virtual_joysticks_are_joysticks() {
        let base = |setup| DeviceConfig {
            name: "uinput test".into(),
            vid: 0x1234,
            pid: 0x0001,
            serial: None,
            interface: 0,
            setup,
        };
        let handbrake = base(Setup::Handbrake(crate::config::HandbrakeSetup {
            input: crate::config::HandbrakeInput::Axis { index: 0, size: 1 },
            calibration_min: 0,
            calibration_max: 255,
            deadzone: 0.0,
            curve: vec![[0.0, 0.0], [1.0, 1.0]],
            mode: "analog".into(),
            button_threshold: 50.0,
        }));
        let sig = Signature {
            bytes: vec![ByteMatch {
                index: 0,
                mask: 1,
                value: 1,
            }],
        };
        let sequential = base(Setup::Sequential {
            up: sig.clone(),
            down: sig,
        });
        for config in [handbrake, sequential] {
            classified_as_joystick(&config)
                .unwrap_or_else(|props| panic!("{:?}\n{props}", config.setup));
        }
    }
}
