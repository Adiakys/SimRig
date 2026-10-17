//! Installing and operating plugins' drivers through `simrig-helper`.
//!
//! The app never runs as root. For plugins installed system-wide the helper runs through
//! `pkexec` (the user is asked for a password, kept for a while by polkit); for development
//! plugins (explicit directories) it runs as the user with a private state directory, so the
//! whole flow can be exercised without privileges. Plugins in the user's home may not install
//! drivers.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use simrig_driver::{
    DriverContract, DriverStatus, HookContext, InstalledDriver, Registry, StateLayout, run_hook,
};

use crate::error::HostError;
use crate::manifest::{Manifest, PluginLocation};

const STATUS_TIMEOUT: Duration = Duration::from_secs(10);
/// The helper run through pkexec for system plugins.
pub(crate) const SYSTEM_HELPER: &str = "/usr/libexec/simrig/simrig-helper";
/// pkexec: the user dismissed the dialog or is not authorized.
const PKEXEC_DENIED: [i32; 2] = [126, 127];

#[derive(Clone, Debug)]
pub(crate) struct DriverManager {
    helper: Option<PathBuf>,
    dev_state: StateLayout,
    dev_roots: Vec<PathBuf>,
}

/// How to run the helper for one plugin.
#[derive(Clone, Debug)]
pub(crate) struct PrivilegedRunner {
    program: PathBuf,
    prefix: Vec<OsString>,
    plugin: String,
}

impl PrivilegedRunner {
    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.prefix);
        command
    }

    /// Runs a helper action and returns its stdout.
    fn run(&self, args: &[String]) -> Result<String, HostError> {
        let output = self.command().args(args).output()?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let pkexec = self.program.file_name().is_some_and(|n| n == "pkexec");
        match output.status.code() {
            Some(code) if pkexec && PKEXEC_DENIED.contains(&code) => Err(HostError::Denied(stderr)),
            _ => Err(HostError::Driver(stderr)),
        }
    }

    pub(crate) fn privileged(
        &self,
        operation: &str,
        args: &std::collections::BTreeMap<String, String>,
    ) -> Result<String, HostError> {
        let mut command = vec![
            "privileged".to_string(),
            self.plugin.clone(),
            operation.to_string(),
        ];
        command.extend(args.iter().map(|(k, v)| format!("{k}={v}")));
        self.run(&command)
    }
}

impl DriverManager {
    pub(crate) fn new(
        helper: Option<PathBuf>,
        dev_state: PathBuf,
        dev_roots: Vec<PathBuf>,
    ) -> Self {
        Self {
            helper,
            dev_state: StateLayout::new(dev_state),
            dev_roots,
        }
    }

    /// Where the drivers of plugins at `location` are recorded; `None` if they may not have any.
    pub(crate) fn layout(&self, location: PluginLocation) -> Option<StateLayout> {
        match location {
            PluginLocation::System => Some(StateLayout::system()),
            PluginLocation::Explicit => Some(self.dev_state.clone()),
            PluginLocation::User => None,
        }
    }

    pub(crate) fn runner(
        &self,
        plugin: &str,
        location: PluginLocation,
    ) -> Result<PrivilegedRunner, HostError> {
        let helper = self
            .helper
            .clone()
            .ok_or_else(|| HostError::Driver("simrig-helper not found".into()))?;
        let (program, mut prefix) = match location {
            // As root, only the helper installed with the system (root-owned, the one the polkit
            // action names): never a development build the user could have replaced.
            PluginLocation::System => {
                let system = PathBuf::from(SYSTEM_HELPER);
                if !system.is_file() {
                    return Err(HostError::Driver(format!(
                        "{SYSTEM_HELPER} is not installed: system plugins cannot install drivers"
                    )));
                }
                (PathBuf::from("pkexec"), vec![system.into_os_string()])
            }
            PluginLocation::Explicit => (helper, Vec::new()),
            PluginLocation::User => {
                return Err(HostError::Denied(format!(
                    "plugin '{plugin}' is installed in your home directory: only system-wide plugins may install drivers"
                )));
            }
        };
        if location == PluginLocation::Explicit {
            for root in &self.dev_roots {
                prefix.push("--plugin-root".into());
                prefix.push(root.clone().into_os_string());
            }
            prefix.push("--state-dir".into());
            prefix.push(self.dev_state.root.clone().into_os_string());
        }
        Ok(PrivilegedRunner {
            program,
            prefix,
            plugin: plugin.into(),
        })
    }

    pub(crate) fn contract(
        manifest: &Manifest,
        dir: &Path,
    ) -> Result<Option<DriverContract>, HostError> {
        match &manifest.driver {
            Some(path) => Ok(Some(DriverContract::load(&dir.join(path))?)),
            None => Ok(None),
        }
    }

    /// Runs the driver's `status` hook as the user. `None` when the plugin has no driver.
    pub(crate) fn status(
        &self,
        manifest: &Manifest,
        dir: &Path,
        location: PluginLocation,
    ) -> Result<Option<DriverStatus>, HostError> {
        let Some(contract) = Self::contract(manifest, dir)? else {
            return Ok(None);
        };
        let Some(path) = &manifest.driver else {
            return Ok(None);
        };
        let driver_dir = dir
            .join(path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| dir.to_path_buf());
        let data_dir = self
            .layout(location)
            .map(|l| l.data_dir(&manifest.id))
            .unwrap_or_else(|| std::env::temp_dir().join("simrig-no-driver"));
        let ctx = HookContext {
            plugin: manifest.id.clone(),
            driver_dir,
            data_dir,
            args: Default::default(),
        };
        let output = run_hook(&ctx, &contract.status, STATUS_TIMEOUT)?;
        serde_json::from_str(output.stdout.trim())
            .map(Some)
            .map_err(|e| HostError::Driver(format!("invalid status from '{}': {e}", manifest.id)))
    }

    pub(crate) fn install(
        &self,
        manifest: &Manifest,
        dir: &Path,
        location: PluginLocation,
    ) -> Result<String, HostError> {
        let contract = Self::contract(manifest, dir)?.ok_or_else(|| {
            HostError::NotFound(format!("plugin '{}' has no driver", manifest.id))
        })?;
        if !contract.requires.is_empty() {
            // Prerequisites go through PackageKit; not implemented yet.
            return Err(HostError::Driver(format!(
                "install these packages first: {}",
                contract.requires.join(", ")
            )));
        }
        self.runner(&manifest.id, location)?
            .run(&["install".into(), manifest.id.clone()])
    }

    pub(crate) fn uninstall(
        &self,
        plugin: &str,
        location: PluginLocation,
    ) -> Result<String, HostError> {
        self.runner(plugin, location)?
            .run(&["uninstall".into(), plugin.into()])
    }

    /// Drivers recorded in the system and development registries.
    pub(crate) fn installed(&self) -> Vec<(PluginLocation, InstalledDriver)> {
        let mut all = Vec::new();
        for location in [PluginLocation::System, PluginLocation::Explicit] {
            if let Some(layout) = self.layout(location) {
                if let Ok(registry) = Registry::load(&layout) {
                    all.extend(registry.drivers.into_iter().map(|d| (location, d)));
                }
            }
        }
        all
    }
}

/// `simrig-helper`: next to the running executable (development builds), then the installed one.
pub(crate) fn find_helper() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        if dir.ends_with("deps") {
            candidates.extend(dir.parent().map(|d| d.join("simrig-helper")));
        }
        candidates.push(dir.join("simrig-helper"));
    }
    candidates.push(PathBuf::from(SYSTEM_HELPER));
    candidates.into_iter().find(|p| p.is_file())
}
