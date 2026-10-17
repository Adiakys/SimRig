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
