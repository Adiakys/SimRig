//! Whether the system is set up for each device as games need it, checked as the user and
//! without touching the device:
//! that its input nodes open, that udev lists it as a joystick, that the kernel driver it needs
//! handles it, and that the driver running is the one installed. Each problem is a [`Finding`]
//! with the facts behind it and, when the plugin's driver has one, the fix.

use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the checks look: the system root, its `modinfo`, and the user's home for Steam's
/// settings (tests use a fake tree).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemRoot {
    pub root: PathBuf,
    pub modinfo: PathBuf,
    pub home: Option<PathBuf>,
}

impl Default for SystemRoot {
    fn default() -> Self {
        Self {
            root: PathBuf::from("/"),
            modinfo: PathBuf::from("modinfo"),
            home: std::env::var_os("HOME").map(PathBuf::from),
        }
    }
}

impl SystemRoot {
    fn path(&self, absolute: &str) -> PathBuf {
        self.root.join(absolute.trim_start_matches('/'))
    }
}

/// A device node and who may open it, as `ls -l` shows it (`crw-rw---- root:input`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub path: String,
    pub mode: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The user cannot open the input nodes games read.
    NoAccess { nodes: Vec<Node> },
    /// udev does not mark any input node of the device as a joystick (`ID_INPUT_JOYSTICK`).
    NotJoystick { node: String, props: Vec<String> },
    /// Another kernel driver, or none, handles the device in place of the one it needs.
    WrongDriver {
        bound: Option<String>,
        expected: String,
    },
    /// The module running is not the one installed (`modinfo` resolves another file).
    OtherCopy {
        module: String,
        loaded: String,
        installed: String,
        file: String,
    },
    /// Steam Input is forced on for a game, which may then get a virtual controller instead.
    SteamInput { app: String, game: String },
    /// Steam Input is on for generic controllers, in every game that does not turn it off.
    SteamGeneric,
}

/// What fixes a problem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fix {
    /// Installing the plugin's driver again: it loads its module again and gives the devices
    /// plugged in back to it.
    Rebind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub problem: Problem,
    pub fix: Option<Fix>,
    /// The user chose to ignore it on this device.
    pub ignored: bool,
}

/// The findings of one device of the rig, ignored ones included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceFindings {
    pub plugin: String,
    pub device: String,
    pub findings: Vec<Finding>,
}

impl DeviceFindings {
    /// Findings not ignored.
    pub fn active(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| !f.ignored)
    }
}

impl Finding {
    /// What identifies this problem on a device, to ignore it: the kind and what it is about,
    /// so a different driver or another game shows it again.
    pub fn key(&self) -> String {
        match &self.problem {
            Problem::NoAccess { .. } => "no-access".into(),
            Problem::NotJoystick { .. } => "not-joystick".into(),
            Problem::WrongDriver { bound, .. } => {
                format!("driver:{}", bound.as_deref().unwrap_or("none"))
            }
            Problem::OtherCopy { loaded, .. } => format!("other-copy:{loaded}"),
            Problem::SteamInput { app, .. } => format!("steam-input:{app}"),
            Problem::SteamGeneric => "steam-input:generic".into(),
        }
    }

    /// Only what can be wanted can be ignored; a device games cannot see is never wanted (a
    /// device not used in games is excluded from the rig instead).
    pub fn ignorable(&self) -> bool {
        matches!(
            self.problem,
            Problem::WrongDriver { .. }
                | Problem::OtherCopy { .. }
                | Problem::SteamInput { .. }
                | Problem::SteamGeneric
        )
    }

    /// Information rather than a warning: it may be wanted, and is not certain to matter.
    pub fn informational(&self) -> bool {
        matches!(
            self.problem,
            Problem::SteamInput { .. } | Problem::SteamGeneric
        )
    }

    pub fn title(&self) -> String {
        match &self.problem {
            Problem::NoAccess { .. } => "Games cannot read this device".into(),
            Problem::NotJoystick { .. } => "Games do not list this device".into(),
            Problem::WrongDriver { bound: None, .. } => "No driver handles this device".into(),
            Problem::WrongDriver { .. } => "Force feedback does not reach games".into(),
            Problem::OtherCopy { .. } => "Another copy of the driver is loaded".into(),
            Problem::SteamInput { game, .. } => format!("Steam Input is on for {game}"),
            Problem::SteamGeneric => "Steam Input is on for generic controllers".into(),
        }
    }

    pub fn explanation(&self) -> String {
        match &self.problem {
            Problem::NoAccess { .. } => {
                "Its input node does not open for this user, so games do not see it. Access is \
                 given to the user signed in at the computer: sessions started remotely or \
                 without systemd-logind do not get it."
                    .into()
            }
            Problem::NotJoystick { .. } => {
                "udev does not classify it as a joystick, so games that find devices through \
                 SDL, every game under Proton included, skip it, and the user signed in does \
                 not get access to it."
                    .into()
            }
            Problem::WrongDriver {
                bound: None,
                expected,
            } => format!(
                "Neither the {expected} driver nor any other handles the device, so games do \
                 not see it."
            ),
            Problem::WrongDriver {
                bound: Some(bound),
                expected,
            } if bound == "hid-generic" => format!(
                "The kernel's generic HID driver handles the device in place of the {expected} \
                 driver: games see its buttons and axes but send forces nowhere."
            ),
            Problem::WrongDriver {
                bound: Some(bound),
                expected,
            } => format!(
                "The {bound} driver handles the device in place of the {expected} driver: \
                 force feedback and the settings here may not work."
            ),
            Problem::OtherCopy { module, .. } => format!(
                "The kernel runs a {module} other than the one installed by SimRig. Settings \
                 and force feedback follow the copy that runs. Remove it to use SimRig's, or \
                 uninstall the driver in Plugins to keep it."
            ),
            Problem::SteamInput { .. } => {
                "Steam can offer the game a virtual controller in place of the device. If the \
                 game does not see it, turn Steam Input off for this game in Steam: \
                 Properties, Controller."
                    .into()
            }
            Problem::SteamGeneric => {
                "Steam can offer games a virtual controller in place of the device. If a game \
                 does not see it, turn Steam Input off for that game in Steam: Properties, \
                 Controller."
                    .into()
            }
        }
    }

    /// The facts, for whoever wants them: nodes, drivers, versions.
    pub fn detail(&self) -> String {
        match &self.problem {
            Problem::NoAccess { nodes, .. } => nodes
                .iter()
                .map(|n| format!("{} {}", n.path, n.mode))
                .collect::<Vec<_>>()
                .join("  "),
            Problem::NotJoystick { node, props } if props.is_empty() => {
                format!("{node}: ID_INPUT_JOYSTICK not set")
            }
            Problem::NotJoystick { node, props } => {
                format!("{node}: ID_INPUT_JOYSTICK not set ({})", props.join(", "))
            }
            Problem::WrongDriver { bound, expected } => format!(
                "bound to {}, expected {expected}",
                bound.as_deref().unwrap_or("no driver")
            ),
            Problem::OtherCopy {
                loaded,
                installed,
                file,
                ..
            } => format!("running {loaded}, installed {installed} ({file})"),
            Problem::SteamInput { app, .. } => format!("app {app}: UseSteamControllerConfig 2"),
            Problem::SteamGeneric => "SteamController_GenericGamepadSupport 1".into(),
        }
    }
}

/// What the plugin and its driver say about a device, for the checks.
#[derive(Clone, Debug, Default)]
pub(crate) struct Expect<'a> {
    /// The kernel driver the device needs.
    pub kernel_driver: Option<&'a str>,
    /// Games read the device's own input nodes (not a virtual joystick).
    pub own_input: bool,
    /// The plugin's driver is installed and running, nothing pending: what it does should show.
    pub driver_ready: bool,
    /// SimRig installed the driver (not the system): the module running must be its own.
    pub driver_ours: bool,
}

/// An input node below a USB device.
#[derive(Debug)]
struct Event {
    name: String,
    /// `major:minor`.
    dev: String,
    abs: bool,
    ff: bool,
}

/// What a USB device has in sysfs: its HID functions with their driver, and its input nodes.
#[derive(Debug, Default)]
struct Tree {
    hid: Vec<(PathBuf, Option<String>)>,
    events: Vec<Event>,
}

fn walk(dir: &Path, depth: usize, tree: &mut Tree) {
    if depth > 5 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        // Links (`driver`, `subsystem`, `port`) lead out of the device.
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if name.starts_with("0003:") {
            tree.hid.push((path.clone(), bound_driver(&path)));
        } else if name.len() > 5
            && name.starts_with("event")
            && name[5..].chars().all(|c| c.is_ascii_digit())
        {
            let caps = |cap: &str| {
                fs::read_to_string(dir.join("capabilities").join(cap)).is_ok_and(|v| {
                    v.split_whitespace()
                        .any(|w| !w.trim_start_matches('0').is_empty())
                })
            };
            tree.events.push(Event {
                dev: read(&path.join("dev")).unwrap_or_default(),
                name,
                abs: caps("abs"),
                ff: caps("ff"),
            });
            continue;
        }
        walk(&path, depth + 1, tree);
    }
}

fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn bound_driver(dir: &Path) -> Option<String> {
    fs::read_link(dir.join("driver"))
        .ok()
        .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
}

/// The properties udev keeps for a character device, `None` when its database is not there.
fn udev_props(root: &SystemRoot, dev: &str) -> Option<Vec<(String, String)>> {
    let text = fs::read_to_string(root.path(&format!("/run/udev/data/c{dev}"))).ok()?;
    Some(
        text.lines()
            .filter_map(|l| l.strip_prefix("E:"))
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    )
}

/// Opens the node as a game does: to read, and to write when it takes force feedback.
fn opens(path: &Path, write: bool) -> bool {
    fs::OpenOptions::new()
        .read(true)
        .write(write)
        .open(path)
        .is_ok()
}

fn node(root: &SystemRoot, absolute: &str) -> Node {
    let path = root.path(absolute);
    let mode = match fs::metadata(&path) {
        Ok(m) => {
            let kind = if m.file_type().is_char_device() {
                'c'
            } else {
                '-'
            };
            let bits = m.permissions().mode();
            let rwx: String = (0..9)
                .map(|i| {
                    if bits & (1 << (8 - i)) == 0 {
                        '-'
                    } else {
                        ['r', 'w', 'x'][i % 3]
                    }
                })
                .collect();
            format!(
                "{kind}{rwx} {}:{}",
                name_of(root, "/etc/passwd", m.uid()),
                name_of(root, "/etc/group", m.gid())
            )
        }
        Err(_) => "missing".into(),
    };
    Node {
        path: absolute.into(),
        mode,
    }
}

/// A user or group name from `/etc/passwd` or `/etc/group`, or the number.
fn name_of(root: &SystemRoot, file: &str, id: u32) -> String {
    fs::read_to_string(root.path(file))
        .ok()
        .and_then(|text| {
            text.lines().find_map(|l| {
                let mut f = l.split(':');
                let name = f.next()?;
                (f.nth(1)? == id.to_string()).then(|| name.to_string())
            })
        })
        .unwrap_or_else(|| id.to_string())
}

/// `modinfo -F <field> <module>`, of the module modprobe would load.
fn modinfo(root: &SystemRoot, args: &[&str]) -> Option<String> {
    let out = Command::new(&root.modinfo).args(args).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !text.is_empty()).then_some(text)
}

/// The checks on one USB device (its sysfs directory).
pub(crate) fn check_device(usb: &Path, root: &SystemRoot, expect: &Expect) -> Vec<Problem> {
    let mut tree = Tree::default();
    walk(usb, 0, &mut tree);
    let mut problems = Vec::new();

    if expect.own_input && !tree.events.is_empty() {
        let props: Vec<Option<Vec<(String, String)>>> = tree
            .events
            .iter()
            .map(|e| udev_props(root, &e.dev))
            .collect();
        let joystick = |p: &Option<Vec<(String, String)>>| {
            p.as_ref()
                .is_some_and(|p| p.iter().any(|(k, v)| k == "ID_INPUT_JOYSTICK" && v == "1"))
        };
        let known = props.iter().any(Option::is_some);
        // The nodes games read: the joysticks, or those with axes when udev cannot tell.
        let games: Vec<&Event> = tree
            .events
            .iter()
            .zip(&props)
            .filter(|(e, p)| if known { joystick(p) } else { e.abs })
            .map(|(e, _)| e)
            .collect();
        if known && games.is_empty() {
            let (event, p) = tree
                .events
                .iter()
                .zip(&props)
                .find(|(e, _)| e.abs)
                .unwrap_or((&tree.events[0], &props[0]));
            problems.push(Problem::NotJoystick {
                node: event.name.clone(),
                props: p
                    .iter()
                    .flatten()
                    .filter(|(k, v)| k.starts_with("ID_INPUT_") && k != "ID_INPUT" && v == "1")
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect(),
            });
        }
        let denied: Vec<Node> = games
            .iter()
            .map(|e| (format!("/dev/input/{}", e.name), e.ff))
            .filter(|(path, ff)| !opens(&root.path(path), *ff))
            .map(|(path, _)| node(root, &path))
            .collect();
        if !denied.is_empty() {
            problems.push(Problem::NoAccess { nodes: denied });
        }
    }

    if let (Some(expected), true) = (expect.kernel_driver, expect.driver_ready) {
        let ours = tree
            .hid
            .iter()
            .find(|(_, d)| d.as_deref() == Some(expected));
        match ours {
            None if !tree.hid.is_empty() => {
                // The function games read, if one has input nodes.
                let bound = tree
                    .hid
                    .iter()
                    .find(|(p, d)| d.is_some() && p.join("input").is_dir())
                    .or_else(|| tree.hid.iter().find(|(_, d)| d.is_some()))
                    .and_then(|(_, d)| d.clone());
                problems.push(Problem::WrongDriver {
                    bound,
                    expected: expected.into(),
                });
            }
            Some((hid, _)) if expect.driver_ours => {
                if let Some(copy) = other_copy(root, hid) {
                    problems.push(copy);
                }
            }
            _ => {}
        }
    }
    problems
}

/// Steam Input settings that may take devices from games: they concern every device games
/// read directly.
pub(crate) fn steam_problems(root: &SystemRoot) -> Vec<Problem> {
    let Some(home) = &root.home else {
        return Vec::new();
    };
    let steam = crate::steam::read(home);
    let mut problems: Vec<Problem> = steam
        .forced
        .into_iter()
        .map(|(app, game)| Problem::SteamInput { app, game })
        .collect();
    if steam.generic {
        problems.push(Problem::SteamGeneric);
    }
    problems
}

/// The module that runs the device, when it is not the file `modinfo` resolves.
fn other_copy(root: &SystemRoot, hid: &Path) -> Option<Problem> {
    let module = fs::canonicalize(hid.join("driver/module"))
        .ok()?
        .file_name()?
        .to_string_lossy()
        .into_owned();
    let running = root.path(&format!("/sys/module/{module}"));
    let loaded = read(&running.join("srcversion"))?;
    let installed = modinfo(root, &["-F", "srcversion", &module])?;
    if loaded == installed {
        return None;
    }
    let version = |v: Option<String>, src: &str| match v {
        Some(v) => format!("{v} ({src})"),
        None => src.to_string(),
    };
    Some(Problem::OtherCopy {
        loaded: version(read(&running.join("version")), &loaded),
        installed: version(modinfo(root, &["-F", "version", &module]), &installed),
        file: modinfo(root, &["-n", &module]).unwrap_or_default(),
        module,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    struct Fake {
        root: PathBuf,
        usb: PathBuf,
    }

    impl Fake {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "simrig-device-checks-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&root);
            let usb = root.join("sys/devices/pci0000:00/usb3/3-4");
            fs::create_dir_all(&usb).unwrap();
            fs::create_dir_all(root.join("dev/input")).unwrap();
            fs::create_dir_all(root.join("run/udev/data")).unwrap();
            Fake { root, usb }
        }

        fn system(&self) -> SystemRoot {
            SystemRoot {
                root: self.root.clone(),
                modinfo: self.root.join("modinfo"),
                home: None,
            }
        }

        /// A HID function bound to `driver` (a module `module`), with an input node.
        fn hid(&self, driver: Option<(&str, &str)>, event: u32, abs: bool, ff: bool) -> PathBuf {
            let hid = self.usb.join("3-4:1.0/0003:0EB7:0020.0005");
            let input = hid.join("input/input23");
            let ev = input.join(format!("event{event}"));
            fs::create_dir_all(&ev).unwrap();
            fs::create_dir_all(input.join("capabilities")).unwrap();
            fs::write(
                input.join("capabilities/abs"),
                if abs { "3\n" } else { "0\n" },
            )
            .unwrap();
            fs::write(
                input.join("capabilities/ff"),
                if ff { "107030000 0\n" } else { "0\n" },
            )
            .unwrap();
            fs::write(ev.join("dev"), format!("13:{}\n", 64 + event)).unwrap();
            let node = self.root.join(format!("dev/input/event{event}"));
            fs::write(&node, "").unwrap();
            if let Some((driver, module)) = driver {
                let d = self.root.join("sys/bus/hid/drivers").join(driver);
                fs::create_dir_all(&d).unwrap();
                let m = self.root.join("sys/module").join(module);
                fs::create_dir_all(&m).unwrap();
                let _ = symlink(&m, d.join("module"));
                symlink(&d, hid.join("driver")).unwrap();
            }
            hid
        }

        fn udev(&self, event: u32, props: &str) {
            fs::write(
                self.root.join(format!("run/udev/data/c13:{}", 64 + event)),
                props,
            )
            .unwrap();
        }

        fn deny(&self, absolute: &str) {
            let path = self.root.join(absolute.trim_start_matches('/'));
            fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn expect() -> Expect<'static> {
        Expect {
            kernel_driver: Some("fanatec"),
            own_input: true,
            driver_ready: true,
            driver_ours: false,
        }
    }

    // Root opens anything: the access checks cannot be seen failing.
    fn root_user() -> bool {
        fs::read_to_string("/proc/self/status").is_ok_and(|s| {
            s.lines()
                .find(|l| l.starts_with("Uid:"))
                .is_some_and(|l| l.split_whitespace().nth(1) == Some("0"))
        })
    }

    #[test]
    fn a_device_games_can_use_has_no_findings() {
        let fake = Fake::new("ok");
        fake.hid(Some(("fanatec", "hid_fanatec")), 7, true, true);
        fake.udev(7, "E:ID_INPUT=1\nE:ID_INPUT_JOYSTICK=1\n");
        assert_eq!(check_device(&fake.usb, &fake.system(), &expect()), vec![]);
    }

    #[test]
    fn input_nodes_that_do_not_open() {
        if root_user() {
            return;
        }
        let fake = Fake::new("access");
        fake.hid(Some(("fanatec", "hid_fanatec")), 7, true, true);
        fake.udev(7, "E:ID_INPUT_JOYSTICK=1\n");
        fake.deny("/dev/input/event7");
        let problems = check_device(&fake.usb, &fake.system(), &expect());
        let [Problem::NoAccess { nodes }] = problems.as_slice() else {
            panic!("{problems:?}");
        };
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].path, "/dev/input/event7");
        assert!(nodes[0].mode.starts_with("----------"), "{:?}", nodes[0]);
    }

    #[test]
    fn a_device_udev_does_not_call_a_joystick() {
        let fake = Fake::new("joystick");
        fake.hid(Some(("fanatec", "hid_fanatec")), 7, true, false);
        fake.udev(7, "E:ID_INPUT=1\nE:ID_INPUT_KEY=1\nE:ID_INPUT_JOYSTICK=0\n");
        let problems = check_device(&fake.usb, &fake.system(), &expect());
        assert_eq!(
            problems,
            vec![Problem::NotJoystick {
                node: "event7".into(),
                props: vec!["ID_INPUT_KEY=1".into()],
            }]
        );
        // Games of a virtual joystick do not read the device's nodes.
        let virtual_joystick = Expect {
            own_input: false,
            ..expect()
        };
        assert_eq!(
            check_device(&fake.usb, &fake.system(), &virtual_joystick),
            vec![]
        );
    }

    #[test]
    fn without_the_udev_database_nothing_is_guessed() {
        let fake = Fake::new("noudev");
        fake.hid(Some(("fanatec", "hid_fanatec")), 7, true, false);
        assert_eq!(check_device(&fake.usb, &fake.system(), &expect()), vec![]);
    }

    #[test]
    fn the_generic_driver_in_place_of_the_needed_one() {
        let fake = Fake::new("generic");
        fake.hid(Some(("hid-generic", "hid_generic")), 7, true, false);
        fake.udev(7, "E:ID_INPUT_JOYSTICK=1\n");
        let problems = check_device(&fake.usb, &fake.system(), &expect());
        assert_eq!(
            problems,
            vec![Problem::WrongDriver {
                bound: Some("hid-generic".into()),
                expected: "fanatec".into(),
            }]
        );
        // A driver not installed yet already says so.
        let pending = Expect {
            driver_ready: false,
            ..expect()
        };
        assert_eq!(check_device(&fake.usb, &fake.system(), &pending), vec![]);
    }

    #[test]
    fn another_copy_of_the_module_runs() {
        let fake = Fake::new("copy");
        fake.hid(Some(("fanatec", "hid_fanatec")), 7, true, true);
        fake.udev(7, "E:ID_INPUT_JOYSTICK=1\n");
        let module = fake.root.join("sys/module/hid_fanatec");
        fs::write(module.join("srcversion"), "AAAA\n").unwrap();
        let modinfo = fake.root.join("modinfo");
        let script = |srcversion: &str| {
            format!(
                "#!/bin/sh\ncase \"$1 $2\" in\n\"-F srcversion\") echo {srcversion} ;;\n\
                 \"-F version\") echo 0.11 ;;\n\"-n hid_fanatec\") echo /lib/modules/k/updates/dkms/hid-fanatec.ko.zst ;;\n\
                 *) exit 1 ;;\nesac\n"
            )
        };
        fs::write(&modinfo, script("BBBB")).unwrap();
        fs::set_permissions(&modinfo, fs::Permissions::from_mode(0o755)).unwrap();
        let ours = Expect {
            driver_ours: true,
            ..expect()
        };
        let problems = check_device(&fake.usb, &fake.system(), &ours);
        assert_eq!(
            problems,
            vec![Problem::OtherCopy {
                module: "hid_fanatec".into(),
                loaded: "AAAA".into(),
                installed: "0.11 (BBBB)".into(),
                file: "/lib/modules/k/updates/dkms/hid-fanatec.ko.zst".into(),
            }]
        );
        // Someone else's driver is not SimRig's to compare.
        assert_eq!(check_device(&fake.usb, &fake.system(), &expect()), vec![]);
        fs::write(&modinfo, script("AAAA")).unwrap();
        assert_eq!(check_device(&fake.usb, &fake.system(), &ours), vec![]);
    }

    #[test]
    fn keys_tell_what_to_show_again() {
        let finding = |problem| Finding {
            problem,
            fix: None,
            ignored: false,
        };
        let generic = finding(Problem::WrongDriver {
            bound: Some("hid-generic".into()),
            expected: "fanatec".into(),
        });
        assert_eq!(generic.key(), "driver:hid-generic");
        assert!(generic.ignorable());
        let access = finding(Problem::NoAccess { nodes: vec![] });
        assert!(!access.ignorable());
        assert!(!access.title().contains('—') && !access.explanation().contains('—'));
    }
}
