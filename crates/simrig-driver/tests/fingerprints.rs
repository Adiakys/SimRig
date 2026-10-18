//! Every official driver keeps the digest of its files in `driver.fingerprint`, so that no
//! change slips by without deciding whether drivers already installed need it: those are
//! offered an update only when `version` or `revision` in `driver.toml` changes.
//!
//! After changing a driver, record its files again:
//!
//!   SIMRIG_UPDATE_FINGERPRINTS=1 cargo test -p simrig-driver --test fingerprints

use std::fs;
use std::path::Path;

const FILE: &str = "driver.fingerprint";
/// Built at packaging time, not in the repository.
const SKIP: &[&str] = &["bin", FILE];

fn recorded(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
}

#[test]
fn driver_changes_are_weighed_against_their_revision() {
    let plugins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let update = std::env::var_os("SIMRIG_UPDATE_FINGERPRINTS").is_some();
    let mut changed = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(&plugins).unwrap().flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let driver = entry.path().join("driver");
        if !driver.join("driver.toml").is_file() {
            continue;
        }
        let digest = simrig_driver::digest(&driver, SKIP).unwrap();
        let path = driver.join(FILE);
        if recorded(&path).as_deref() == Some(digest.as_str()) {
            continue;
        }
        if update {
            fs::write(
                &path,
                format!(
                    "# Digest of this driver's files: see crates/simrig-driver/tests/fingerprints.rs.\n{digest}\n"
                ),
            )
            .unwrap();
        } else {
            changed.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    assert!(
        changed.is_empty(),
        "the drivers of {changed:?} changed. If drivers already installed need the change \
         (udev rules, a service, hooks run from the installed copy: uninstall and privileged \
         operations), raise `revision` in their driver.toml (back to 0 with a new `version`). \
         The generic USB service is built from plugins/generic-usb/src: raise its revision when \
         it changes too. Then record the files: SIMRIG_UPDATE_FINGERPRINTS=1 cargo test -p \
         simrig-driver --test fingerprints"
    );
}
