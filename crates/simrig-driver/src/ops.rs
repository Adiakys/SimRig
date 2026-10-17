//! Installing, removing and running privileged operations of drivers. `simrig-helper` calls
//! these as root for system plugins; development builds call them as the user.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::contract::{DriverContract, valid_id};
use crate::error::DriverError;
use crate::hooks::{HookContext, run_hook};
use crate::layout::{StateLayout, copy_dir};
use crate::registry::{InstallState, InstalledDriver, Registry, remove_if_exists};

const INSTALL_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(5 * 60);

fn context(
    layout: &StateLayout,
    plugin: &str,
    driver_dir: &Path,
    args: BTreeMap<String, String>,
) -> HookContext {
    HookContext {
        plugin: plugin.into(),
        driver_dir: driver_dir.to_path_buf(),
        data_dir: layout.data_dir(plugin),
        args,
    }
}

fn check_plugin_id(plugin: &str) -> Result<(), DriverError> {
    if valid_id(plugin) {
        Ok(())
    } else {
        Err(DriverError::InvalidArgument {
            name: "plugin".into(),
            message: format!("invalid id '{plugin}'"),
        })
    }
}

/// Installs the driver whose contract is `contract_path` (inside the plugin), for `plugin`.
/// The driver directory is copied into the state directory first, so a failed or interrupted
/// install can always be uninstalled.
pub fn install(
    layout: &StateLayout,
    plugin: &str,
    contract_path: &Path,
) -> Result<String, DriverError> {
    check_plugin_id(plugin)?;
    let contract = DriverContract::load(contract_path)?;
    let driver_dir = contract_path.parent().unwrap_or(Path::new("."));
    let contract_name = contract_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let copy = layout.driver_copy(plugin);
    if copy.exists() {
        fs::remove_dir_all(&copy)?;
    }
    copy_dir(driver_dir, &copy)?;
    fs::create_dir_all(layout.data_dir(plugin))?;
    write_uninstall_script(layout, plugin, &contract.uninstall)?;
    write_remove_script(layout)?;

    let mut registry = Registry::load(layout)?;
    let mut entry = InstalledDriver {
        plugin: plugin.into(),
        driver: contract.id.clone(),
        version: contract.version.clone(),
        state: InstallState::Installing,
        installed_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        contract: contract_name,
    };
    registry.upsert(entry.clone());
    registry.save(layout)?;

    let result = run_hook(
        &context(layout, plugin, &copy, BTreeMap::new()),
        &contract.install,
        INSTALL_TIMEOUT,
    );
    entry.state = if result.is_ok() {
        InstallState::Installed
    } else {
        InstallState::Failed
    };
    registry.upsert(entry);
    registry.save(layout)?;
    result.map(|o| o.stdout)
}

/// Removes the driver of `plugin` using the installed copy: works even if the plugin and the
/// app are gone. Leaves nothing behind once the last driver is removed.
pub fn uninstall(layout: &StateLayout, plugin: &str) -> Result<String, DriverError> {
    check_plugin_id(plugin)?;
    let mut registry = Registry::load(layout)?;
    let entry = registry
        .find(plugin)
        .cloned()
        .ok_or_else(|| DriverError::NotInstalled(plugin.into()))?;
    let copy = layout.driver_copy(plugin);
    let contract = DriverContract::load(&copy.join(&entry.contract))?;
    let output = run_hook(
        &context(layout, plugin, &copy, BTreeMap::new()),
        &contract.uninstall,
        INSTALL_TIMEOUT,
    )?;

    fs::remove_dir_all(layout.plugin_dir(plugin))?;
    registry.remove(plugin);
    registry.save(layout)?;
    if registry.drivers.is_empty() {
        remove_if_exists(&layout.remove_script())?;
        let _ = fs::remove_dir(layout.drivers());
        let _ = fs::remove_dir(&layout.root);
    }
    Ok(output.stdout)
}

/// Runs a privileged operation declared by the installed driver of `plugin`.
pub fn privileged(
    layout: &StateLayout,
    plugin: &str,
    operation: &str,
    args: BTreeMap<String, String>,
) -> Result<String, DriverError> {
    check_plugin_id(plugin)?;
    let registry = Registry::load(layout)?;
    let entry = registry
        .find(plugin)
        .filter(|e| e.state == InstallState::Installed)
        .ok_or_else(|| DriverError::NotInstalled(plugin.into()))?;
    let copy = layout.driver_copy(plugin);
    let contract = DriverContract::load(&copy.join(&entry.contract))?;
    let op = contract.operation(operation)?;
    op.validate_args(&args)?;
    Ok(run_hook(
        &context(layout, plugin, &copy, args),
        &op.exec,
        OPERATION_TIMEOUT,
    )?
    .stdout)
}

fn write_executable(path: &Path, text: &str) -> Result<(), DriverError> {
    fs::write(path, text)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

fn write_uninstall_script(
    layout: &StateLayout,
    plugin: &str,
    hook: &str,
) -> Result<(), DriverError> {
    // `plugin` and `hook` are validated: plain identifiers and relative paths, safe in a script.
    let script = format!(
        "#!/bin/sh\n\
         # Generated by simrig: removes the driver of plugin '{plugin}'.\n\
         set -e\n\
         cd \"$(dirname \"$0\")/driver\"\n\
         SIMRIG_PLUGIN='{plugin}' SIMRIG_DRIVER_DIR=\"$PWD\" SIMRIG_DRIVER_DATA=\"$(dirname \"$PWD\")/data\" ./{hook}\n"
    );
    write_executable(&layout.uninstall_script(plugin), &script)
}

fn write_remove_script(layout: &StateLayout) -> Result<(), DriverError> {
    let script = r#"#!/bin/sh
# Generated by simrig: removes drivers installed by simrig, also after simrig itself was
# uninstalled. Usage: sudo remove-drivers --all | <plugin>...
set -e
root="$(cd "$(dirname "$0")" && pwd)"
if [ "$#" -eq 0 ]; then
    echo "usage: $0 --all | <plugin>..."
    echo "installed:"; ls "$root/drivers" 2>/dev/null || true
    exit 2
fi
if [ "$1" = "--all" ]; then
    set -- $(ls "$root/drivers" 2>/dev/null)
fi
for plugin in "$@"; do
    sh "$root/drivers/$plugin/uninstall.sh"
    rm -rf "$root/drivers/$plugin"
    echo "removed $plugin"
done
if [ -z "$(ls -A "$root/drivers" 2>/dev/null)" ]; then
    rm -rf "$root"
fi
"#;
    fs::create_dir_all(&layout.root)?;
    write_executable(&layout.remove_script(), script)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    struct Fixture {
        dir: std::path::PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn fixture(name: &str) -> (Fixture, StateLayout, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("simrig-driver-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let driver = dir.join("plugin/driver");
        fs::create_dir_all(driver.join("hooks")).unwrap();
        fs::write(
            driver.join("driver.toml"),
            "id = 'fake'\nversion = '1.0'\nsummary = 'fake'\ninstall = 'hooks/install'\nuninstall = 'hooks/uninstall'\nstatus = 'hooks/status'\n\
             [[privileged]]\nname = 'echo'\nexec = 'hooks/echo'\nargs = { word = '[a-z]+' }\n",
        )
        .unwrap();
        let hook = |name: &str, body: &str| {
            write_executable(
                &driver.join("hooks").join(name),
                &format!("#!/bin/sh\nset -e\n{body}\n"),
            )
            .unwrap();
        };
        hook(
            "install",
            "touch \"$SIMRIG_DRIVER_DATA/installed\"; echo installed",
        );
        hook(
            "uninstall",
            "rm -f \"$SIMRIG_DRIVER_DATA/installed\"; echo removed",
        );
        hook("status", "echo '{\"installed\": true}'");
        hook("echo", "echo \"$SIMRIG_PLUGIN:$SIMRIG_ARG_WORD\"");
        let layout = StateLayout::new(dir.join("state"));
        (
            Fixture { dir: dir.clone() },
            layout,
            driver.join("driver.toml"),
        )
    }

    #[test]
    fn install_operate_uninstall_leaves_nothing() {
        let (_f, layout, contract) = fixture("cycle");
        assert_eq!(
            install(&layout, "fake", &contract).unwrap().trim(),
            "installed"
        );
        assert!(layout.data_dir("fake").join("installed").exists());
        let entry = Registry::load(&layout)
            .unwrap()
            .find("fake")
            .cloned()
            .unwrap();
        assert_eq!(entry.state, InstallState::Installed);

        let args = BTreeMap::from([("word".to_string(), "hello".to_string())]);
        assert_eq!(
            privileged(&layout, "fake", "echo", args).unwrap().trim(),
            "fake:hello"
        );
        let bad = BTreeMap::from([("word".to_string(), "x; rm".to_string())]);
        assert!(matches!(
            privileged(&layout, "fake", "echo", bad),
            Err(DriverError::InvalidArgument { .. })
        ));

        assert_eq!(uninstall(&layout, "fake").unwrap().trim(), "removed");
        assert!(!layout.root.exists(), "state directory left behind");
        assert!(matches!(
            uninstall(&layout, "fake"),
            Err(DriverError::NotInstalled(_))
        ));
    }

    #[test]
    fn standalone_script_removes_drivers() {
        let (_f, layout, contract) = fixture("script");
        install(&layout, "fake", &contract).unwrap();
        let status = Command::new("sh")
            .arg(layout.remove_script())
            .arg("--all")
            .status()
            .unwrap();
        assert!(status.success());
        assert!(!layout.root.exists());
        assert!(Registry::load(&layout).unwrap().drivers.is_empty());
    }

    #[test]
    fn failed_install_can_be_uninstalled() {
        let (_f, layout, contract) = fixture("failed");
        let hooks = contract.parent().unwrap().join("hooks");
        write_executable(&hooks.join("install"), "#!/bin/sh\necho boom >&2\nexit 3\n").unwrap();
        assert!(matches!(
            install(&layout, "fake", &contract),
            Err(DriverError::HookFailed { .. })
        ));
        assert_eq!(
            Registry::load(&layout).unwrap().find("fake").unwrap().state,
            InstallState::Failed
        );
        assert!(privileged(&layout, "fake", "echo", BTreeMap::new()).is_err());
        uninstall(&layout, "fake").unwrap();
        assert!(!layout.root.exists());
    }
}
