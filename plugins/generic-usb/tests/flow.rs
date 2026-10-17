//! End to end: a fake USB device given to the generic plugin, set up by moving it, then read
//! live as a standard component. The driver runs in development mode (no root): setups are
//! saved in its data directory.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use simrig_host::{Host, HostEvent, HostOptions, Target};
use simrig_model::{Gear, ProcedureState, Step, Value};

const KEY: &str = "usb:1234:0001";
const PORT: &str = "9-1";

struct Env {
    root: PathBuf,
}

impl Env {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("simrig-generic-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("usb")).unwrap();
        Env { root }
    }

    fn device(&self) -> PathBuf {
        self.root.join("usb").join(PORT)
    }

    /// A USB device no plugin knows, whose current report is in its fake report file.
    fn plug(&self, report: &str) {
        let dev = self.device();
        fs::create_dir_all(&dev).unwrap();
        fs::write(dev.join("idVendor"), "1234\n").unwrap();
        fs::write(dev.join("idProduct"), "0001\n").unwrap();
        fs::write(dev.join("product"), "USB Game Controller\n").unwrap();
        self.report(report);
    }

    fn report(&self, report: &str) {
        fs::write(
            self.device().join("simrig-fake-report"),
            format!("{report}\n"),
        )
        .unwrap();
    }

    fn data(&self) -> PathBuf {
        self.root.join("state/drivers/generic-usb/data")
    }

    fn start(&self) -> Host {
        let exec = PathBuf::from(env!("CARGO_BIN_EXE_simrig-plugin-generic-usb"));
        let bin_dir = exec.parent().unwrap().to_path_buf();
        Host::start(&HostOptions {
            plugin_dirs: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))],
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

/// `simrig-helper` from the same build; built on demand when only this package is tested.
fn helper(bin_dir: &Path) -> PathBuf {
    let path = bin_dir.join("simrig-helper");
    if !path.is_file() {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-q", "-p", "simrig-helper"])
            .status()
            .unwrap();
        assert!(status.success(), "cannot build simrig-helper");
    }
    path
}

/// Waits for the procedure to show a step whose title satisfies `want`.
fn wait_step(host: &mut Host, session: &str, what: &str, want: impl Fn(&Step) -> bool) -> Step {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        for event in host.poll() {
            if let HostEvent::ProcedureUpdated {
                session: s,
                state: ProcedureState::Step(step),
            } = event
            {
                if s == session && want(&step) {
                    return step;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no step: {what}");
}

/// Assigns the fake device to the plugin, with its driver installed.
fn assign(host: &mut Host) -> String {
    host.install_driver("generic-usb").unwrap();
    host.choose_plugin(KEY, Some("generic-usb")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        host.poll();
        if let Some((_, device)) = host.devices().into_iter().find(|(p, _)| p == "generic-usb") {
            return device.id;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the generic plugin did not get the device");
}

fn start_setup(host: &mut Host, device: &str) -> (String, Step) {
    let target = Target::Device {
        plugin: "generic-usb".into(),
        device: device.into(),
    };
    let ids: Vec<String> = host
        .procedures(&target)
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, ["setup"]);
    let (session, state) = host.start_procedure(&target, "setup").unwrap();
    let ProcedureState::Step(step) = state else {
        panic!("{state:?}")
    };
    (session, step)
}

fn submit(
    host: &mut Host,
    session: &str,
    action: &str,
    values: &[(&str, Value)],
) -> ProcedureState {
    let values: BTreeMap<String, Value> = values
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    host.submit_procedure(session, action, values).unwrap()
}

fn component(host: &mut Host, suffix: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        host.poll();
        if let Ok(c) = host.find(suffix) {
            return c.address.clone();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no component {suffix}");
}

#[test]
fn shifter_set_up_by_moving_it() {
    let env = Env::new("shifter");
    // Byte 0 is a counter that moves by itself; byte 2 holds the gear bits.
    env.plug("00 80 00");
    let mut host = env.start();
    let device = assign(&mut host);
    let (session, step) = start_setup(&mut host, &device);
    assert_eq!(step.title, "What is this device?");

    submit(
        &mut host,
        &session,
        "next",
        &[
            ("kind", Value::Choice("h_shifter".into())),
            ("gears", Value::Int(4)),
            ("reverse_position", Value::Choice("right_down".into())),
        ],
    );
    // At rest, with noise on byte 0.
    for noise in ["01 80 00", "02 80 00", "03 80 00"] {
        env.report(noise);
        std::thread::sleep(Duration::from_millis(150));
    }
    wait_step(&mut host, &session, "1st gear", |s| {
        s.title.contains("1st gear")
    });

    // Each gear: one bit of byte 2, held still. A gear already taken is refused.
    let gears = [
        ("2nd gear", "01 80 01"),
        ("3rd gear", "02 80 02"),
        ("4th gear", "03 80 04"),
        ("reverse", "00 80 08"),
    ];
    for (next, report) in gears {
        env.report(report);
        wait_step(&mut host, &session, next, |s| s.title.contains(next));
    }
    env.report("01 80 40");
    let summary = wait_step(&mut host, &session, "summary", |s| {
        s.title == "Check the mapping"
    });
    assert!(
        format!("{:?}", summary.elements).contains("byte 2 bit 6"),
        "{:?}",
        summary.elements
    );

    let state = submit(&mut host, &session, "save", &[]);
    assert!(
        matches!(state, ProcedureState::Finished { .. }),
        "{state:?}"
    );
    let saved = fs::read_to_string(env.data().join("devices/1234-0001.toml")).unwrap();
    assert!(saved.contains("kind = \"h_shifter\""), "{saved}");

    // Now a shifter: the gear follows the device.
    let shifter = component(&mut host, "shifter");
    env.report("02 80 02");
    std::thread::sleep(Duration::from_millis(100));
    let inputs = host.read_inputs(&shifter).unwrap();
    assert_eq!(inputs.get("gear"), Some(&Value::Gear(Gear::Forward(2))));
    env.report("03 80 40");
    std::thread::sleep(Duration::from_millis(100));
    let inputs = host.read_inputs(&shifter).unwrap();
    assert_eq!(inputs.get("gear"), Some(&Value::Gear(Gear::Reverse)));
    env.report("00 80 00");
    std::thread::sleep(Duration::from_millis(100));
    let inputs = host.read_inputs(&shifter).unwrap();
    assert_eq!(inputs.get("gear"), Some(&Value::Gear(Gear::Neutral)));
}

#[test]
fn handbrake_axis_measured_by_pulling_it() {
    let env = Env::new("handbrake");
    // A 16-bit little-endian axis at bytes 1..3.
    env.plug("00 10 00");
    let mut host = env.start();
    let device = assign(&mut host);
    let (session, _) = start_setup(&mut host, &device);
    submit(
        &mut host,
        &session,
        "next",
        &[
            ("kind", Value::Choice("handbrake".into())),
            ("gears", Value::Int(6)),
            ("reverse_position", Value::Choice("right_down".into())),
        ],
    );
    wait_step(&mut host, &session, "pull", |s| {
        s.title.contains("Pull the handbrake")
    });
    for report in ["00 00 04", "00 00 08", "00 f0 0f"] {
        env.report(report);
        std::thread::sleep(Duration::from_millis(100));
    }
    let summary = wait_step(&mut host, &session, "summary", |s| {
        s.title == "Check the mapping"
    });
    assert!(
        format!("{:?}", summary.elements).contains("16-bit axis at byte 1"),
        "{:?}",
        summary.elements
    );
    submit(&mut host, &session, "save", &[]);

    let handbrake = component(&mut host, "handbrake");
    env.report("00 00 08");
    std::thread::sleep(Duration::from_millis(100));
    let inputs = host.read_inputs(&handbrake).unwrap();
    let Some(Value::Axis(axis)) = inputs.get("position") else {
        panic!("{inputs:?}")
    };
    assert_eq!(axis.raw, 0x800);
    assert!(axis.value > 0.4 && axis.value < 0.55, "{axis:?}");

    // Settings change at once in the app and are saved into the driver.
    host.set(&handbrake, "deadzone", Value::Float(10.0))
        .unwrap();
    host.save(&handbrake, "deadzone").unwrap();
    let saved = fs::read_to_string(env.data().join("devices/1234-0001.toml")).unwrap();
    assert!(saved.contains("deadzone = 10.0"), "{saved}");
}
