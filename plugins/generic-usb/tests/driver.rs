//! The Generic USB driver's hooks, as root, in a sandbox with stub system commands. They run
//! from the copy of the driver SimRig keeps when it installs it, so everything they need ships
//! inside the driver directory.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use simrig_driver::testing::Sandbox;

const DRIVER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/driver");

/// A copy of the driver, as the helper makes it, with the service binary inside it or not.
fn copy(name: &str, with_service: bool) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "simrig-generic-driver-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    copy_dir(Path::new(DRIVER), &dir);
    if with_service {
        let service = dir.join("bin/simrig-generic-usb-service");
        fs::create_dir_all(service.parent().unwrap()).unwrap();
        fs::write(&service, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&service, fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn sandbox(name: &str, with_service: bool) -> Sandbox {
    let driver = copy(name, with_service);
    let sb = Sandbox::new(&driver, "generic-usb", name, true);
    sb.write(
        "bin/systemctl",
        "#!/bin/sh\necho \"systemctl $*\" >> \"$SIMRIG_TEST_ROOT/log\"\n",
    );
    fs::set_permissions(sb.file("bin/systemctl"), fs::Permissions::from_mode(0o755)).unwrap();
    sb
}

#[test]
fn installs_the_service_shipped_inside_the_driver() {
    let sb = sandbox("install", true);
    sb.install();
    assert!(sb.data.join("bin/simrig-generic-usb-service").is_file());
    let unit = sb.read("etc/systemd/system/simrig-generic-usb@.service");
    assert!(unit.contains("simrig-generic-usb-service"), "{unit}");
    assert!(sb.log().contains("systemctl daemon-reload"), "{}", sb.log());
    assert!(sb.status().installed);
}

#[test]
fn a_driver_without_its_service_says_so() {
    let sb = sandbox("missing", false);
    let (ok, _, err) = sb.hook("install");
    assert!(!ok);
    assert!(err.contains("missing from the driver"), "{err}");
}
