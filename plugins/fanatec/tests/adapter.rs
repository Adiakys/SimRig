//! End to end through the host: a fake CSL DD, as hid-fanatecff exposes it in sysfs (driver
//! bound, tuning menu, a Formula V2 rim) with fake inputs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use simrig_host::{Host, HostEvent, HostOptions, Target};
use simrig_model::{ProcedureState, RimShape, Value};

const HID: &str = "1-2/1-2:1.0/0003:0EB7:0020.0001";

struct Env {
    root: PathBuf,
}

impl Env {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("simrig-fanatec-e2e-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("usb")).unwrap();
        // The plugin, with its configuration in the sandbox.
        let plugin = root.join("plugins/fanatec");
        fs::create_dir_all(&plugin).unwrap();
        let manifest = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/plugin.toml"))
            .unwrap()
            .replace(
                "driver = \"driver/driver.toml\"",
                &format!(
                    "driver = \"driver/driver.toml\"\nargs = [\"--config\", \"{}\"]",
                    root.join("fanatec.toml").display()
                ),
            );
        fs::write(plugin.join("plugin.toml"), manifest).unwrap();
        // The driver, without the sources (nothing is built without root). A symbolic link
        // would be refused: drivers must live inside their plugin.
        copy_dir(
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/driver")),
            &plugin.join("driver"),
        );
        Env { root }
    }

    /// A CSL DD; `bound`: the fanatec driver handles it.
    fn plug(&self, bound: bool) {
        let usb = self.root.join("usb/1-2");
        fs::create_dir_all(&usb).unwrap();
        fs::write(usb.join("idVendor"), "0eb7\n").unwrap();
        fs::write(usb.join("idProduct"), "0020\n").unwrap();
        fs::write(usb.join("product"), "FANATEC CSL DD\n").unwrap();
        let hid = self.root.join("usb").join(HID);
        fs::create_dir_all(&hid).unwrap();
        if !bound {
            return;
        }
        let driver = self.root.join("drivers/fanatec");
        fs::create_dir_all(&driver).unwrap();
        std::os::unix::fs::symlink(&driver, hid.join("driver")).unwrap();
        // Like a real CSL DD: a second HID device bound to the driver, with raw HID only,
        // listed before the one with the settings.
        let raw = self.root.join("usb/1-2/1-2:1.0/0003:0EB7:0020.0000");
        fs::create_dir_all(raw.join("hidraw")).unwrap();
        std::os::unix::fs::symlink(&driver, raw.join("driver")).unwrap();
        for (attr, value) in [
            ("range", "900"),
            ("wheel_id", "0x000a"),
            ("fw_version", "47"),
            ("display", ""),
        ] {
            fs::write(hid.join(attr), format!("{value}\n")).unwrap();
        }
        let tuning = hid.join("ftec_tuning/0003:0EB7:0020.0001");
        fs::create_dir_all(&tuning).unwrap();
        for (attr, value) in [
            ("FF", "75"),
            ("NDP", "10"),
            ("NFR", "0"),
            ("NIN", "0"),
            ("SPR", "100"),
            ("DPR", "100"),
            ("SHO", "50"),
            ("ACP", "1"),
            ("FOR", "100"),
            ("FEI", "100"),
            ("FFS", "0"),
            ("INT", "1"),
            ("FUL", "50"),
            ("DRI", "0"),
            ("BLI", "101"),
            ("brF", "50"),
            ("SEN", "900"),
            ("SLOT", "1"),
            ("RESET", ""),
        ] {
            fs::write(tuning.join(attr), format!("{value}\n")).unwrap();
        }
        let wheel = hid.join("ftec_wheel/0003:0EB7:0020.0001");
        fs::create_dir_all(&wheel).unwrap();
        fs::write(wheel.join("name"), "ClubSport Steering Wheel Formula V2\n").unwrap();
        // Steering at three quarters (right), pedals at rest, one button pressed.
        self.input("abs 0 49151 0 65535\nabs 1 0 0 1023\nabs 2 0 0 1023\nkey 288 1\nkey 289 0\n");
    }

    fn hid(&self, attr: &str) -> PathBuf {
        self.root.join("usb").join(HID).join(attr)
    }

    fn input(&self, text: &str) {
        fs::write(self.hid("simrig-fake-input"), text).unwrap();
    }

    fn start(&self) -> Host {
        let exec = PathBuf::from(env!("CARGO_BIN_EXE_simrig-plugin-fanatec"));
        let bin_dir = exec.parent().unwrap().to_path_buf();
        Host::start(&HostOptions {
            plugin_dirs: vec![self.root.join("plugins/fanatec")],
            default_dirs: false,
            exec_dirs: vec![bin_dir.clone()],
            usb_root: self.root.join("usb"),
            config_dir: self.root.join("config"),
            data_dir: self.root.join("data"),
            dev_state_dir: self.root.join("state"),
            helper: Some(helper(&bin_dir)),
            ..HostOptions::default()
        })
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().flatten() {
        let name = entry.file_name();
        if name == "hid-fanatecff" {
            continue;
        }
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &to.join(name));
        } else {
            fs::copy(entry.path(), to.join(name)).unwrap();
        }
    }
}

fn helper(bin_dir: &Path) -> PathBuf {
    let path = bin_dir.join("simrig-helper");
    if !path.is_file() {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-q", "-p", "simrig-helper"])
            .status()
            .unwrap();
        assert!(status.success());
    }
    path
}

fn until(host: &mut Host, what: &str, done: impl Fn(&mut Host) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        host.poll();
        if done(host) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out: {what}");
}

fn reason(host: &mut Host, address: &str, item: &str) -> String {
    match &host.describe(address).unwrap().item(item).unwrap().support {
        simrig_model::Support::Unsupported(u) => u.explain(),
        simrig_model::Support::Supported(_) => String::new(),
    }
}

#[test]
fn a_csl_dd_with_a_formula_rim() {
    let env = Env::new("csl-dd");
    env.plug(true);
    let mut host = env.start();
    until(&mut host, "components", |h| h.components().len() == 2);

    let base = host.find("base").unwrap().address.clone();
    let wheel = host.find("wheel").unwrap().address.clone();
    assert_eq!(host.find(&base).unwrap().description.name, "CSL DD");
    let w = host.describe(&wheel).unwrap().clone();
    assert_eq!(w.name, "ClubSport Steering Wheel Formula V2");
    assert_eq!(w.appearance.rim, Some(RimShape::Formula));

    // 3/4 of the axis over a 900° rotation: 225° to the right.
    let inputs = host.read_inputs(&base).unwrap();
    let Some(Value::Float(angle)) = inputs.get("angle") else {
        panic!("{inputs:?}")
    };
    assert!((angle - 225.0).abs() < 0.1, "{angle}");
    assert_eq!(inputs.get("firmware"), Some(&Value::Text("47".into())));
    let rim = host.read_inputs(&wheel).unwrap();
    assert_eq!(rim.get("rim_id"), Some(&Value::Text("0x000a".into())));
    assert_eq!(rim.get("buttons"), Some(&Value::Buttons(vec![true, false])));

    // Settings go straight to the driver.
    let spec = host.param(&base, "rotation").unwrap();
    assert_eq!(spec.value, Value::Float(900.0));
    assert!(matches!(spec.ty, simrig_model::ParamType::Float { max, .. } if max == 2520.0));
    host.set(&base, "ffb_strength", Value::Float(60.0)).unwrap();
    assert_eq!(
        fs::read_to_string(env.hid("ftec_tuning/0003:0EB7:0020.0001/FF")).unwrap(),
        "60\n"
    );
    host.set(&base, "rotation", Value::Float(540.0)).unwrap();
    assert_eq!(fs::read_to_string(env.hid("range")).unwrap(), "540\n");
    host.set(&base, "custom.tuning_for", Value::Float(80.0))
        .unwrap();
    assert_eq!(
        fs::read_to_string(env.hid("ftec_tuning/0003:0EB7:0020.0001/FOR")).unwrap(),
        "80\n"
    );
    host.set(&wheel, "analog_paddle_mode", Value::Choice("3".into()))
        .unwrap();
    assert_eq!(
        fs::read_to_string(env.hid("ftec_tuning/0003:0EB7:0020.0001/ACP")).unwrap(),
        "3\n"
    );

    // The rotation is saved as the driver's start-up option (development: in its data).
    host.install_driver("fanatec").unwrap();
    fs::write(env.hid("range"), "540\n").unwrap();
    host.save(&base, "rotation").unwrap();
    let saved = env.root.join("state/drivers/fanatec/data/init_range");
    assert_eq!(fs::read_to_string(saved).unwrap(), "540\n");

    // What the driver cannot do stays visible, with its reason.
    assert!(reason(&mut host, &base, "set_center").contains("centering"));
    assert!(reason(&mut host, &base, "angular_velocity").contains("driver"));

    // A rim swap: the McLaren GT3.
    fs::write(env.hid("wheel_id"), "0x310b\n").unwrap();
    fs::write(
        env.hid("ftec_wheel/0003:0EB7:0020.0001/name"),
        "CSL Elite Steering Wheel McLaren GT3\n",
    )
    .unwrap();
    until(&mut host, "rim swap", |h| {
        h.find("wheel")
            .is_ok_and(|c| c.description.appearance.rim == Some(RimShape::Gt))
    });
}

#[test]
fn pedals_are_mapped_by_pressing_them() {
    let env = Env::new("pedals");
    env.plug(true);
    let mut host = env.start();
    until(&mut host, "components", |h| h.components().len() == 2);
    // No pedals until they are mapped: the base offers the mapping.
    assert!(host.find("pedals").is_err());
    let base = host.find("base").unwrap().address.clone();
    let ids: Vec<String> = host
        .describe(&base)
        .unwrap()
        .procedures
        .iter()
        .map(|p| p.id.clone())
        .collect();
    assert_eq!(ids, ["map-pedals"]);

    let target = Target::Component(base);
    let (session, _) = host.start_procedure(&target, "map-pedals").unwrap();
    host.submit_procedure(&session, "next", BTreeMap::new())
        .unwrap();
    let step = |host: &mut Host, title: &str| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            for event in host.poll() {
                if let HostEvent::ProcedureUpdated {
                    state: ProcedureState::Step(s),
                    ..
                } = event
                {
                    if s.title.contains(title) {
                        return;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("no step {title}");
    };
    // Throttle on axis 1, brake on axis 2 (pressed towards its minimum), no clutch.
    env.input("abs 0 32767 0 65535\nabs 1 1023 0 1023\nabs 2 0 0 1023\n");
    step(&mut host, "brake");
    env.input("abs 0 32767 0 65535\nabs 1 0 0 1023\nabs 2 1023 0 1023\n");
    step(&mut host, "clutch");
    host.submit_procedure(&session, "skip", BTreeMap::new())
        .unwrap();
    let state = host
        .submit_procedure(&session, "save", BTreeMap::new())
        .unwrap();
    assert!(
        matches!(state, ProcedureState::Finished { .. }),
        "{state:?}"
    );
    assert!(
        fs::read_to_string(env.root.join("fanatec.toml"))
            .unwrap()
            .contains("throttle")
    );

    until(&mut host, "pedals mapped", |h| {
        h.find("pedals")
            .is_ok_and(|c| c.description.pedals.len() == 2)
    });
    env.input("abs 0 32767 0 65535\nabs 1 512 0 1023\nabs 2 0 0 1023\n");
    let pedals = host.find("pedals").unwrap().address.clone();
    let inputs = host.read_inputs(&pedals).unwrap();
    let Some(Value::Axis(throttle)) = inputs.get("throttle.position") else {
        panic!("{inputs:?}")
    };
    assert!((throttle.value - 0.5).abs() < 0.01, "{throttle:?}");
}

#[test]
fn without_the_driver_everything_says_so() {
    let env = Env::new("unbound");
    env.plug(false);
    let mut host = env.start();
    until(&mut host, "components", |h| h.components().len() == 1);
    let base = host.find("base").unwrap().address.clone();
    assert!(reason(&mut host, &base, "rotation").contains("driver to be installed"));
    assert!(reason(&mut host, &base, "angle").contains("hid-fanatec"));
}

/// The manifest claims exactly the devices the bundled driver handles.
#[test]
fn manifest_matches_the_driver() {
    let header = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/driver/hid-fanatecff/hid-ftec.h"
    ))
    .unwrap();
    let mut from_driver: Vec<u16> = header
        .lines()
        .filter(|l| l.starts_with("#define") && l.contains("_DEVICE_ID"))
        .filter_map(|l| l.split_whitespace().nth(2))
        .filter_map(|v| {
            u16::from_str_radix(v.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
        })
        .collect();
    from_driver.sort();
    let manifest = simrig_host::Manifest::load(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/plugin.toml"
    )))
    .unwrap();
    let mut claimed: Vec<u16> = manifest.matching.usb.iter().filter_map(|m| m.pid).collect();
    claimed.sort();
    assert_eq!(claimed, from_driver);
    let mut modelled: Vec<u16> = simrig_plugin_fanatec::models::MODELS
        .iter()
        .map(|m| m.pid)
        .collect();
    modelled.sort();
    assert_eq!(modelled, from_driver);
}
