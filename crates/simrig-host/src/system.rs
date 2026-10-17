//! What the wizard checks first: distribution, kernel, Secure Boot, session.

use std::fs;
use std::path::Path;

const SECURE_BOOT_VAR: &str =
    "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemInfo {
    /// `PRETTY_NAME` of `/etc/os-release`.
    pub distro: Option<String>,
    /// `ID` and `ID_LIKE`, to map package names.
    pub distro_ids: Vec<String>,
    pub kernel: Option<String>,
    /// `None` on BIOS systems or when it cannot be read.
    pub secure_boot: Option<bool>,
    /// `wayland`, `x11`…
    pub session: Option<String>,
    pub desktop: Option<String>,
    pub systemd: bool,
    pub pkexec: bool,
}

impl SystemInfo {
    pub fn detect() -> Self {
        let os_release = fs::read_to_string("/etc/os-release")
            .or_else(|_| fs::read_to_string("/usr/lib/os-release"))
            .unwrap_or_default();
        let field = |key: &str| {
            os_release
                .lines()
                .find_map(|l| l.strip_prefix(&format!("{key}=")))
                .map(|v| v.trim_matches('"').to_string())
        };
        let mut distro_ids: Vec<String> = field("ID").into_iter().collect();
        distro_ids.extend(
            field("ID_LIKE")
                .unwrap_or_default()
                .split_whitespace()
                .map(String::from),
        );
        let env = |key: &str| std::env::var(key).ok().filter(|v| !v.is_empty());
        SystemInfo {
            distro: field("PRETTY_NAME"),
            distro_ids,
            kernel: fs::read_to_string("/proc/sys/kernel/osrelease")
                .ok()
                .map(|k| k.trim().to_string()),
            // The variable is 4 bytes of attributes followed by the value.
            secure_boot: fs::read(SECURE_BOOT_VAR)
                .ok()
                .and_then(|b| b.get(4).map(|v| *v == 1)),
            session: env("XDG_SESSION_TYPE"),
            desktop: env("XDG_CURRENT_DESKTOP"),
            systemd: Path::new("/run/systemd/system").is_dir(),
            pkexec: ["/usr/bin/pkexec", "/bin/pkexec"]
                .iter()
                .any(|p| Path::new(p).is_file()),
        }
    }
}
