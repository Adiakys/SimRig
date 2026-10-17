//! End to end through the host: a fake CSL DD, as hid-fanatecff exposes it in sysfs (driver
//! bound, tuning menu, a Formula V2 rim) with fake inputs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use simrig_host::Target;
use simrig_host::testing::{PluginEnv, reason, step, until};
use simrig_model::{ProcedureState, RimShape, Value};

const HID: &str = "1-2:1.0/0003:0EB7:0020.0001";

struct Env {
    env: PluginEnv,
    hid: PathBuf,
}

impl Env {
    fn new(name: &str) -> Self {
        let env = PluginEnv::new(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            "fanatec",
            Path::new(env!("CARGO_BIN_EXE_simrig-plugin-fanatec")),
            name,
            &["hid-fanatecff"],
        );
        let hid = env.root.join("usb/1-2").join(HID);
        Env { env, hid }
    }

    /// A CSL DD; `bound`: the fanatec driver handles it.
    fn plug(&self, bound: bool) {
        self.env
            .plug("1-2", 0x0eb7, 0x0020, "FANATEC CSL DD", HID, None);
        if !bound {
            return;
        }
        self.env.bind(&self.hid, "fanatec");
        // Like the real driver: the HID device with the settings has the input device.
        fs::create_dir_all(self.hid.join("input")).unwrap();
        // Like a real CSL DD: a second HID device bound to the driver, with raw HID only,
        // listed before the one with the settings.
        let raw = self.env.root.join("usb/1-2/1-2:1.0/0003:0EB7:0020.0000");
        fs::create_dir_all(raw.join("hidraw")).unwrap();
        self.env.bind(&raw, "fanatec");
        let hid = &self.hid;
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
        self.hid.join(attr)
    }

    fn input(&self, text: &str) {
        self.env.input(&self.hid, text);
    }

    fn start(&self) -> simrig_host::Host {
        self.env.start()
    }
}

#[test]
fn a_csl_dd_with_a_formula_rim() {
    let env = Env::new("csl-dd");
    env.plug(true);
    let mut host = env.start();
    until(&mut host, "components", |h| h.components().len() == 3);

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
    let saved = env.env.driver_data().join("init_range");
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

/// The pedals plugged into a base are recognised (throttle ABS_Z, brake ABS_RZ, clutch ABS_Y,
/// released at the top of their travel, like on a real CSL DD). Mapping them by hand, for
/// pedals that respond wrong, takes the direction from where each pedal is pressed to.
#[test]
fn pedals_on_the_base_are_recognised() {
    let env = Env::new("pedals");
    env.plug(true);
    // Released; the throttle halfway.
    let released =
        "abs 0 32767 0 65535\nabs 1 65535 0 65535\nabs 2 65535 0 65535\nabs 5 65535 0 65535\n";
    env.input(
        "abs 0 32767 0 65535\nabs 1 65535 0 65535\nabs 2 32767 0 65535\nabs 5 65535 0 65535\n",
    );
    let mut host = env.start();
    until(&mut host, "components", |h| h.components().len() == 3);
    let pedals = host.find("pedals").unwrap().address.clone();
    assert_eq!(host.find(&pedals).unwrap().description.pedals.len(), 3);
    let position = |host: &mut simrig_host::Host, pedal: &str| {
        let inputs = host.read_inputs(&pedals).unwrap();
        match inputs.get(&format!("{pedal}.position")) {
            Some(Value::Axis(a)) => a.value,
            other => panic!("{other:?}"),
        }
    };
    assert!((position(&mut host, "throttle") - 0.5).abs() < 0.01);
    assert!(position(&mut host, "brake") < 0.01);

    // Recognised: mapping by hand is offered, not needed to add the base.
    let base = host.find("base").unwrap().address.clone();
    let map = host.describe(&base).unwrap().procedures[0].clone();
    assert_eq!(map.id, "map-pedals");
    assert!(!map.setup);

    // By hand: throttle and brake swapped, as for pedals plugged in the other way round.
    env.input(released);
    let (session, _) = host
        .start_procedure(&Target::Component(base), "map-pedals")
        .unwrap();
    host.submit_procedure(&session, "next", BTreeMap::new())
        .unwrap();
    env.input("abs 0 32767 0 65535\nabs 1 65535 0 65535\nabs 2 65535 0 65535\nabs 5 0 0 65535\n");
    step(&mut host, "brake");
    env.input("abs 0 32767 0 65535\nabs 1 65535 0 65535\nabs 2 0 0 65535\nabs 5 65535 0 65535\n");
    step(&mut host, "clutch");
    env.input(released);
    host.submit_procedure(&session, "skip", BTreeMap::new())
        .unwrap();
    let state = host
        .submit_procedure(&session, "save", BTreeMap::new())
        .unwrap();
    assert!(
        matches!(state, ProcedureState::Finished { .. }),
        "{state:?}"
    );
    let saved = fs::read_to_string(env.env.config()).unwrap();
    assert!(
        saved.contains("[models.0020.pedals.throttle]\ncode = 5\ninverted = true"),
        "{saved}"
    );
    until(&mut host, "mapped by hand", |h| {
        h.find("pedals")
            .is_ok_and(|c| c.description.pedals.len() == 2)
    });
    // Pressing the pedal now on ABS_RZ moves the throttle.
    env.input("abs 0 32767 0 65535\nabs 1 65535 0 65535\nabs 2 65535 0 65535\nabs 5 0 0 65535\n");
    assert!(position(&mut host, "throttle") > 0.99);
}

#[test]
fn without_the_driver_everything_says_so() {
    let env = Env::new("unbound");
    env.plug(false);
    let mut host = env.start();
    until(&mut host, "components", |h| h.components().len() == 2);
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

/// Until the base sends its tuning menu (it does when the menu is used on the wheel), the driver
/// shows it as zeros and refuses writes, the rotation included: SimRig says what to do, reports
/// no invalid values, and offers the settings as soon as the menu arrives.
#[test]
fn a_tuning_menu_not_received_yet_says_what_to_do() {
    let env = Env::new("unread");
    env.plug(true);
    for attr in ["SLOT", "ACP", "FF"] {
        fs::write(
            env.hid(&format!("ftec_tuning/0003:0EB7:0020.0001/{attr}")),
            "0\n",
        )
        .unwrap();
    }
    let mut host = env.start();
    until(&mut host, "components", |h| h.components().len() == 3);
    let base = host.find("base").unwrap().address.clone();
    let wheel = host.find("wheel").unwrap().address.clone();
    for (address, item) in [
        (&base, "rotation"),
        (&base, "ffb_strength"),
        (&wheel, "analog_paddle_mode"),
    ] {
        assert!(
            reason(&mut host, address, item).contains("tuning menu on the wheel"),
            "{item}"
        );
    }
    // Custom parameters too.
    let slot = host
        .describe(&base)
        .unwrap()
        .custom_param("tuning_slot")
        .unwrap()
        .clone();
    match slot.availability {
        simrig_model::Support::Unsupported(u) => {
            assert!(u.explain().contains("tuning menu on the wheel"))
        }
        other => panic!("{other:?}"),
    }
    assert!(host.problems().is_empty(), "{:?}", host.problems());

    // The menu used on the wheel: the base sends it, and the settings show up by themselves.
    for (attr, value) in [("SLOT", "2"), ("ACP", "1"), ("FF", "100")] {
        fs::write(
            env.hid(&format!("ftec_tuning/0003:0EB7:0020.0001/{attr}")),
            format!("{value}\n"),
        )
        .unwrap();
    }
    until(&mut host, "settings available", |h| {
        h.find("base").is_ok_and(|c| {
            matches!(
                c.description.item("rotation").map(|i| &i.support),
                Some(simrig_model::Support::Supported(_))
            )
        })
    });
}

/// A driver just bound creates its attributes, and udev gives the user its event node, a moment
/// after the device shows up: what comes later shows up too, without restarting the app.
#[test]
fn what_the_driver_creates_later_shows_up() {
    let env = Env::new("later");
    env.plug(true);
    fs::remove_file(env.hid("range")).unwrap();
    fs::remove_file(env.hid("simrig-fake-input")).unwrap();
    let mut host = env.start();
    until(&mut host, "components", |h| h.components().len() == 3);
    let base = host.find("base").unwrap().address.clone();
    assert!(!reason(&mut host, &base, "rotation").is_empty());

    fs::write(env.hid("range"), "900\n").unwrap();
    env.input(
        "abs 0 49151 0 65535\nabs 1 65535 0 65535\nabs 2 65535 0 65535\nabs 5 65535 0 65535\n",
    );
    until(&mut host, "rotation and inputs", |h| {
        h.find("base").is_ok_and(|c| {
            matches!(
                c.description.item("rotation").map(|i| &i.support),
                Some(simrig_model::Support::Supported(_))
            )
        })
    });
    let base = host.find("base").unwrap().address.clone();
    let inputs = host.read_inputs(&base).unwrap();
    assert!(
        matches!(inputs.get("angle"), Some(Value::Float(a)) if (a - 225.0).abs() < 0.1),
        "{inputs:?}"
    );
}

/// SimRig gets the tuning menu by itself: it switches the menu's mode (the base answers with the
/// whole menu, in the other mode and on slot 1), switches it back, and selects the slot the user
/// was on.
#[test]
fn the_tuning_menu_is_asked_for_and_the_slot_kept() {
    let env = Env::new("unlock");
    env.plug(true);
    let menu = env.hid("ftec_tuning/0003:0EB7:0020.0001");
    fs::write(menu.join("SLOT"), "0\n").unwrap();
    fs::write(menu.join("advanced_mode"), "0\n").unwrap();
    // The user was on slot 2.
    fs::write(env.env.root.join("fanatec.slots"), "0020 2\n").unwrap();
    let mut host = env.start();

    // The base: answers the first switch with its menu, now in standard mode on slot 1.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while fs::read_to_string(menu.join("advanced_mode")).unwrap() != "1\n" {
        assert!(
            std::time::Instant::now() < deadline,
            "the menu was not asked for"
        );
        host.poll();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    fs::write(menu.join("advanced_mode"), "0\n").unwrap();
    fs::write(menu.join("SLOT"), "1\n").unwrap();

    // Switched back to advanced, and back on slot 2.
    until(&mut host, "slot 2 again", |_| {
        fs::read_to_string(menu.join("SLOT")).unwrap() == "2\n"
    });
    assert_eq!(
        fs::read_to_string(menu.join("advanced_mode")).unwrap(),
        "1\n"
    );
    let base = host.find("base").unwrap().address.clone();
    assert!(reason(&mut host, &base, "rotation").is_empty());
}
