use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::DriverError;

/// `driver.toml`: how a driver is installed, checked and removed.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriverContract {
    pub id: String,
    pub version: String,
    pub summary: String,
    #[serde(default)]
    pub license: Option<String>,
    /// Hooks: executables relative to the driver directory, run without a shell.
    pub install: String,
    pub uninstall: String,
    pub status: String,
    #[serde(default)]
    pub upgrade: Option<String>,
    /// Prerequisites (abstract package names) the app installs before `install`.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Operations the adapter may request at runtime.
    #[serde(default)]
    pub privileged: Vec<PrivilegedOp>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivilegedOp {
    pub name: String,
    pub exec: String,
    /// Argument name → regular expression the whole value must match.
    #[serde(default)]
    pub args: BTreeMap<String, String>,
}

/// What a driver's `status` hook prints, as JSON.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriverStatus {
    pub installed: bool,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub needs_reboot: bool,
    /// An equivalent driver is provided by the system (in-kernel or a distro package): simrig
    /// installs nothing and never removes it.
    #[serde(default)]
    pub provided_by_system: bool,
    /// Whether the current user can use the device without privileges (the hook runs as the
    /// user). Drivers must grant it at install, e.g. with udev `uaccess` rules: the app never
    /// needs root to read inputs, change settings or test force feedback.
    #[serde(default)]
    pub user_access: Option<bool>,
    #[serde(default)]
    pub message: Option<String>,
}

pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Hook paths are plain relative paths inside the driver directory.
fn valid_hook_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-/".contains(c))
}

fn anchored(pattern: &str) -> Result<Regex, regex::Error> {
    Regex::new(&format!("^(?:{pattern})$"))
}

impl DriverContract {
    pub fn load(path: &Path) -> Result<Self, DriverError> {
        let invalid = |message: String| DriverError::Invalid {
            path: path.to_path_buf(),
            message,
        };
        let text = fs::read_to_string(path).map_err(|e| invalid(e.to_string()))?;
        let contract: DriverContract = toml::from_str(&text).map_err(|e| invalid(e.to_string()))?;
        contract.validate().map_err(invalid)?;
        Ok(contract)
    }

    fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.id) {
            return Err(format!("id '{}' must use a-z, 0-9, '-' or '_'", self.id));
        }
        let mut hooks = vec![&self.install, &self.uninstall, &self.status];
        hooks.extend(self.upgrade.iter());
        hooks.extend(self.privileged.iter().map(|op| &op.exec));
        if let Some(bad) = hooks.iter().find(|h| !valid_hook_path(h)) {
            return Err(format!("hook path '{bad}' must be a plain relative path"));
        }
        let mut names: Vec<&str> = Vec::new();
        for op in &self.privileged {
            if !valid_id(&op.name) || names.contains(&op.name.as_str()) {
                return Err(format!(
                    "privileged operation name '{}' is invalid or repeated",
                    op.name
                ));
            }
            names.push(&op.name);
            for (arg, pattern) in &op.args {
                if !valid_id(arg) {
                    return Err(format!(
                        "argument name '{arg}' must use a-z, 0-9, '-' or '_'"
                    ));
                }
                anchored(pattern).map_err(|e| format!("argument '{arg}': {e}"))?;
            }
        }
        Ok(())
    }

    pub fn operation(&self, name: &str) -> Result<&PrivilegedOp, DriverError> {
        self.privileged
            .iter()
            .find(|op| op.name == name)
            .ok_or_else(|| DriverError::UnknownOperation(name.into()))
    }
}

impl PrivilegedOp {
    /// Every declared argument must be present and fully match its pattern; nothing else is
    /// accepted.
    pub fn validate_args(&self, args: &BTreeMap<String, String>) -> Result<(), DriverError> {
        let bad = |name: &str, message: &str| DriverError::InvalidArgument {
            name: name.into(),
            message: message.into(),
        };
        if let Some(extra) = args.keys().find(|k| !self.args.contains_key(*k)) {
            return Err(bad(extra, "not declared"));
        }
        for (name, pattern) in &self.args {
            let value = args.get(name).ok_or_else(|| bad(name, "missing"))?;
            let re = anchored(pattern).map_err(|e| bad(name, &e.to_string()))?;
            if !re.is_match(value) {
                return Err(bad(name, "value not allowed"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract(extra: &str) -> Result<DriverContract, String> {
        let text = format!(
            "id = 'sim-driver'\nversion = '1'\nsummary = 's'\ninstall = 'hooks/install'\nuninstall = 'hooks/uninstall'\nstatus = 'hooks/status'\n{extra}"
        );
        let c: DriverContract = toml::from_str(&text).map_err(|e| e.to_string())?;
        c.validate()?;
        Ok(c)
    }

    #[test]
    fn validates_hooks_and_args() {
        let c = contract(
            "[[privileged]]\nname = 'save-setting'\nexec = 'hooks/save'\nargs = { key = '[a-z.]+', value = '[0-9]{1,4}' }",
        )
        .unwrap();
        let op = c.operation("save-setting").unwrap();
        let args = |k: &str, v: &str| {
            BTreeMap::from([
                ("key".to_string(), k.to_string()),
                ("value".to_string(), v.to_string()),
            ])
        };
        assert!(op.validate_args(&args("base.rotation", "900")).is_ok());
        // patterns are anchored: no partial matches
        assert!(
            op.validate_args(&args("base.rotation; rm -rf /", "900"))
                .is_err()
        );
        assert!(op.validate_args(&args("base", "90000")).is_err());
        assert!(
            op.validate_args(&BTreeMap::from([("key".to_string(), "a".to_string())]))
                .is_err()
        );
        assert!(c.operation("other").is_err());
    }

    #[test]
    fn rejects_escaping_hook_paths() {
        assert!(contract("upgrade = '../x'").is_err());
        assert!(contract("upgrade = '/usr/bin/x'").is_err());
        assert!(contract("upgrade = 'hooks/$(x)'").is_err());
        assert!(contract("upgrade = 'hooks/upgrade'").is_ok());
    }
}
