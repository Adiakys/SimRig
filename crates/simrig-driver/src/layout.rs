use std::fs;
use std::path::{Path, PathBuf};

/// Where installed drivers are recorded when simrig-helper runs as root.
pub const SYSTEM_STATE_DIR: &str = "/var/lib/simrig";

/// Files of the installation registry:
///
/// ```text
/// <root>/installed.toml               registry
/// <root>/remove-drivers               standalone removal script
/// <root>/drivers/<plugin>/driver/     copy of the installed driver (hooks included)
/// <root>/drivers/<plugin>/data/       the driver's own data (SIMRIG_DRIVER_DATA)
/// <root>/drivers/<plugin>/uninstall.sh
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateLayout {
    pub root: PathBuf,
}

impl StateLayout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn system() -> Self {
        Self::new(SYSTEM_STATE_DIR)
    }

    pub fn registry(&self) -> PathBuf {
        self.root.join("installed.toml")
    }

    pub fn remove_script(&self) -> PathBuf {
        self.root.join("remove-drivers")
    }

    pub fn drivers(&self) -> PathBuf {
        self.root.join("drivers")
    }

    pub fn plugin_dir(&self, plugin: &str) -> PathBuf {
        self.drivers().join(plugin)
    }

    pub fn driver_copy(&self, plugin: &str) -> PathBuf {
        self.plugin_dir(plugin).join("driver")
    }

    pub fn data_dir(&self, plugin: &str) -> PathBuf {
        self.plugin_dir(plugin).join("data")
    }

    pub fn uninstall_script(&self, plugin: &str) -> PathBuf {
        self.plugin_dir(plugin).join("uninstall.sh")
    }
}

/// Effective user id of this process, from `/proc/self/status`.
pub fn effective_uid() -> Option<u32> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("Uid:"))?;
    line.split_whitespace().nth(2)?.parse().ok()
}

/// Copies a directory tree, keeping file permissions. Symlinks are not followed.
pub(crate) fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// A digest of the files of a driver directory: paths, contents and executable bits. VCS files
/// (`.git`) and the top-level entries in `skip` are left out. The repository's tests keep one
/// per driver to notice changes that may need a new `revision` (see `driver.fingerprint`).
pub fn digest(dir: &Path, skip: &[&str]) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;

    fn walk(dir: &Path, rel: &str, skip: &[&str], hasher: &mut Sha256) -> std::io::Result<()> {
        let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let kind = entry.file_type()?;
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if file_name == ".git" || (rel.is_empty() && skip.contains(&file_name.as_str())) {
                continue;
            }
            let name = format!("{rel}/{file_name}");
            if kind.is_dir() {
                hasher.update(format!("d {name}\0"));
                walk(&entry.path(), &name, &[], hasher)?;
            } else if kind.is_file() {
                let exec = entry.metadata()?.permissions().mode() & 0o111 != 0;
                let data = fs::read(entry.path())?;
                hasher.update(format!("f {name} {exec} {}\0", data.len()));
                hasher.update(&data);
            }
        }
        Ok(())
    }

    let mut hasher = Sha256::new();
    walk(dir, "", skip, &mut hasher)?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[cfg(test)]
mod digest_tests {
    use super::*;

    #[test]
    fn a_copy_has_the_digest_of_its_source_and_changes_show() {
        let root = std::env::temp_dir().join(format!("simrig-digest-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let src = root.join("src");
        fs::create_dir_all(src.join("hooks")).unwrap();
        fs::write(src.join("driver.toml"), "id = 'x'").unwrap();
        fs::write(src.join("hooks/install"), "#!/bin/sh").unwrap();
        let copy = root.join("copy");
        copy_dir(&src, &copy).unwrap();
        assert_eq!(digest(&src, &[]).unwrap(), digest(&copy, &[]).unwrap());

        use std::os::unix::fs::PermissionsExt;
        let hook = src.join("hooks/install");
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        assert_ne!(digest(&src, &[]).unwrap(), digest(&copy, &[]).unwrap());
        copy_dir(&src, &copy).unwrap();
        assert_eq!(digest(&src, &[]).unwrap(), digest(&copy, &[]).unwrap());
        fs::write(&hook, "#!/bin/sh\necho new").unwrap();
        assert_ne!(digest(&src, &[]).unwrap(), digest(&copy, &[]).unwrap());
        // Skipped entries do not count.
        assert_eq!(
            digest(&src, &["hooks"]).unwrap(),
            digest(&copy, &["hooks"]).unwrap()
        );
        let _ = fs::remove_dir_all(&root);
    }
}
