//! Updates of the app itself. The releases on GitHub say which version is the newest; its
//! package for the way SimRig was installed (`lib/simrig/package-format`, written by the
//! packages and the archive) is downloaded, checked against the release's SHA256SUMS and
//! installed by `simrig-update` with the system's package tool, through pkexec: polkit asks for
//! the password every time.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use slint::ComponentHandle;

use crate::ui;

/// The releases, newest first.
const RELEASES: &str = "https://api.github.com/repos/Adiakys/SimRig/releases?per_page=30";
/// The updater, as in the polkit action `org.simrig.update`.
const UPDATER: &str = "/usr/libexec/simrig/simrig-update";
/// Attached by the release build after the packages: a release without it is still building.
const SUMS: &str = "SHA256SUMS";
/// pkexec: the user dismissed the dialog or is not authorized.
const PKEXEC_DENIED: [i32; 2] = [126, 127];
/// Packages are a few tens of MB: anything far bigger is not one.
const MAX_DOWNLOAD: u64 = 512 << 20;

/// The version running, as the release build stamps it.
pub fn current() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| Version::new(0, 0, 0))
}

/// How SimRig was installed: decides which file of a release updates it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Deb,
    Rpm,
    Arch,
    /// The .tar.gz with its install script.
    Tarball,
}

impl Format {
    fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "deb" => Some(Self::Deb),
            "rpm" => Some(Self::Rpm),
            "archlinux" => Some(Self::Arch),
            "tarball" => Some(Self::Tarball),
            _ => None,
        }
    }

    /// The format of the running app, from `lib/simrig/package-format` next to its `bin`. None
    /// for development builds and copies installed by hand, unless `SIMRIG_PACKAGE_FORMAT`
    /// names one (development).
    pub fn installed() -> Option<Self> {
        if let Some(format) = std::env::var("SIMRIG_PACKAGE_FORMAT").ok().as_deref() {
            return Self::parse(format);
        }
        let exe = std::env::current_exe().ok()?;
        let prefix = exe.parent()?.parent()?;
        Self::parse(&fs::read_to_string(prefix.join("lib/simrig/package-format")).ok()?)
    }

    /// The file of a release in this format (names from packaging/build-packages.sh).
    fn matches(self, asset: &str) -> bool {
        match self {
            Self::Deb => asset.ends_with("_amd64.deb"),
            Self::Rpm => asset.ends_with(".x86_64.rpm"),
            Self::Arch => asset.ends_with("-x86_64.pkg.tar.zst"),
            Self::Tarball => asset.ends_with("-x86_64-linux.tar.gz"),
        }
    }
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    html_url: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

/// A release newer than the running app.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: Version,
    pub prerelease: bool,
    /// The release page, with its notes.
    pub page: String,
    assets: Vec<Asset>,
}

impl Release {
    pub fn asset(&self, format: Format) -> Option<&Asset> {
        self.assets.iter().find(|a| format.matches(&a.name))
    }

    fn sums(&self) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == SUMS)
    }

    /// A release to show the update window with, for screenshots: nothing to download.
    pub fn preview(version: Version) -> Self {
        let names = [
            "simrig_preview_amd64.deb",
            "simrig-preview.x86_64.rpm",
            "simrig-preview-x86_64.pkg.tar.zst",
            "simrig-preview-x86_64-linux.tar.gz",
        ];
        Self {
            prerelease: !version.pre.is_empty(),
            page: format!("https://github.com/Adiakys/SimRig/releases/tag/v{version}"),
            version,
            assets: names
                .iter()
                .map(|n| Asset {
                    name: n.to_string(),
                    browser_download_url: String::new(),
                    size: 0,
                })
                .collect(),
        }
    }
}

/// The newest release above `current` in GitHub's list: published, with its packages attached
/// (the build attaches them some minutes after publishing) and, unless `prereleases`, a final
/// version.
pub fn newest(json: &str, current: &Version, prereleases: bool) -> Result<Option<Release>> {
    let list: Vec<ApiRelease> =
        serde_json::from_str(json).context("unexpected answer from GitHub")?;
    Ok(list
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let version = Version::parse(r.tag_name.trim_start_matches('v')).ok()?;
            Some(Release {
                prerelease: r.prerelease || !version.pre.is_empty(),
                version,
                page: r.html_url,
                assets: r.assets,
            })
        })
        .filter(|r| prereleases || !r.prerelease)
        .filter(|r| r.sums().is_some())
        .max_by(|a, b| a.version.cmp(&b.version))
        .filter(|r| r.version > *current))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        // A stalled download ends instead of waiting forever.
        .timeout_recv_body(Some(Duration::from_secs(15 * 60)))
        .user_agent(format!("SimRig/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Asks GitHub for the newest release above the running app.
pub fn check(prereleases: bool) -> Result<Option<Release>> {
    let text = agent()
        .get(RELEASES)
        .header("Accept", "application/vnd.github+json")
        .call()
        .context("cannot reach GitHub")?
        .body_mut()
        .read_to_string()
        .context("cannot read the answer from GitHub")?;
    newest(&text, &current(), prereleases)
}

/// The checksum SHA256SUMS lists for the asset `name` (`sha256sum` output). GitHub turns the
/// `~` of package versions (`0.1.0~rc.7`) into `.` in asset names, not in the file.
fn listed_sum(sums: &str, name: &str) -> Option<String> {
    let plain = |n: &str| n.replace('~', ".");
    sums.lines().find_map(|line| {
        let (sum, file) = line.split_once(char::is_whitespace)?;
        let file = file.trim_start().trim_start_matches('*');
        (plain(file) == plain(name)).then(|| sum.to_ascii_lowercase())
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Where downloads wait to be installed: the user's cache.
pub fn download_dir() -> PathBuf {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    cache.join("simrig/update")
}

/// Downloads the file of `release` for `format` into `dir` (emptied first), checked against the
/// release's SHA256SUMS; `progress` gets the fraction done. Returns what [`install`] takes: the
/// package, or the archive's extracted folder.
pub fn download(
    release: &Release,
    format: Format,
    dir: &Path,
    mut progress: impl FnMut(f32),
) -> Result<PathBuf> {
    let asset = release
        .asset(format)
        .context("the release has no package for this installation")?;
    let sums = release.sums().context("the release has no SHA256SUMS")?;
    let agent = agent();
    let sums = agent
        .get(&sums.browser_download_url)
        .call()
        .context("cannot download SHA256SUMS")?
        .body_mut()
        .read_to_string()?;
    let expected = listed_sum(&sums, &asset.name)
        .with_context(|| format!("SHA256SUMS does not list {}", asset.name))?;

    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let path = dir.join(&asset.name);
    let part = dir.join(format!("{}.part", asset.name));
    let mut response = agent
        .get(&asset.browser_download_url)
        .call()
        .with_context(|| format!("cannot download {}", asset.name))?;
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD)
        .reader();
    let mut file = File::create(&part)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 64 << 10];
    let total = asset.size.max(1) as f32;
    let mut done = 0u64;
    loop {
        let n = reader
            .read(&mut buf)
            .with_context(|| format!("the download of {} broke off", asset.name))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
        done += n as u64;
        progress((done as f32 / total).min(1.0));
    }
    file.sync_all()?;
    drop(file);
    if hex(&hasher.finalize()) != expected {
        let _ = fs::remove_file(&part);
        bail!("the download does not match the release's checksum");
    }
    fs::rename(&part, &path)?;

    if format != Format::Tarball {
        return Ok(path);
    }
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&path)
        .arg("-C")
        .arg(dir)
        .status()
        .context("cannot run tar")?;
    if !status.success() {
        bail!("cannot extract {}", asset.name);
    }
    let folder = dir.join(asset.name.trim_end_matches(".tar.gz"));
    if !folder.join("install.sh").is_file() {
        bail!("{} has no install.sh", asset.name);
    }
    Ok(folder)
}

/// Why an installation did not happen.
#[derive(Debug, PartialEq)]
pub enum InstallError {
    /// The password was not given.
    Denied,
    Failed(String),
}

/// Installs what [`download`] returned with the system's package tool, through pkexec.
pub fn install(path: &Path) -> Result<(), InstallError> {
    let output = Command::new("pkexec")
        .arg(UPDATER)
        .arg(path)
        .output()
        .map_err(|e| InstallError::Failed(format!("cannot run pkexec: {e}")))?;
    match output.status.code() {
        Some(0) => Ok(()),
        Some(code) if PKEXEC_DENIED.contains(&code) => Err(InstallError::Denied),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // The package tools are verbose: the last lines say what went wrong.
            let lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
            let tail = lines[lines.len().saturating_sub(3)..].join(" ");
            Err(InstallError::Failed(if tail.is_empty() {
                format!("the installation ended with {}", output.status)
            } else {
                tail
            }))
        }
    }
}

static RESTART: AtomicBool = AtomicBool::new(false);

/// Closes the app so that it starts again, once everything is stopped: see [`restart`].
pub fn request_restart() {
    RESTART.store(true, Ordering::SeqCst);
    let _ = slint::quit_event_loop();
}

/// Starts `exe` again if a restart was requested. `exe` is the path the app was started from:
/// once the package replaced it, the running process no longer has a path of its own.
pub fn restart(exe: Option<&Path>) {
    if !RESTART.load(Ordering::SeqCst) {
        return;
    }
    if let Some(exe) = exe
        && let Err(e) = Command::new(exe).spawn()
    {
        eprintln!("cannot start {}: {e}", exe.display());
    }
}

/// Opens a web page in the default browser.
pub fn open_url(url: &str) {
    match Command::new("xdg-open").arg(url).spawn() {
        // Reaped on a thread: xdg-open returns once the browser has the page.
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
        }
        Err(e) => eprintln!("cannot open {url}: {e}"),
    }
}

// ---------------------------------------------------------------------- the app's side

/// Where an update stands, as the update window shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Stage {
    #[default]
    Idle,
    Downloading(f32),
    Installing,
    Installed,
    Failed(String),
}

impl Stage {
    fn busy(&self) -> bool {
        matches!(self, Stage::Downloading(_) | Stage::Installing)
    }
}

/// The update state of the app, on the UI thread.
#[derive(Default)]
pub struct Updates {
    /// The newest release above the running app, once checked.
    pub release: Option<Release>,
    pub checking: bool,
    pub checked: bool,
    /// Why the last check failed.
    pub check_error: Option<String>,
    pub stage: Stage,
    /// Bumped by every check: an answer to an older one is dropped.
    generation: u32,
    /// Checks again while the app stays open.
    timer: Option<slint::Timer>,
}

/// How often an open app looks again.
const RECHECK: Duration = Duration::from_secs(12 * 60 * 60);

impl crate::app::App {
    /// Pre-releases are offered: as chosen in Settings, else when the running version is one.
    pub fn prereleases(&self) -> bool {
        self.settings
            .prereleases
            .unwrap_or_else(|| !current().pre.is_empty())
    }

    /// Checks soon after the start (not to compete with the devices) and then periodically.
    pub fn start_update_checks(&mut self) {
        slint::Timer::single_shot(Duration::from_secs(3), || {
            crate::app::with_app(|a| a.check_updates());
        });
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::Repeated, RECHECK, || {
            crate::app::with_app(|a| a.check_updates());
        });
        self.updates.timer = Some(timer);
    }

    pub fn check_updates(&mut self) {
        if self.updates.stage.busy() || self.updates.stage == Stage::Installed {
            return;
        }
        self.updates.generation += 1;
        self.updates.checking = true;
        self.publish_updates();
        let generation = self.updates.generation;
        let prereleases = self.prereleases();
        std::thread::spawn(move || {
            let result = check(prereleases).map_err(|e| format!("{e:#}"));
            crate::app::with_app(move |a| a.update_checked(generation, result));
        });
    }

    fn update_checked(&mut self, generation: u32, result: Result<Option<Release>, String>) {
        if generation != self.updates.generation {
            return;
        }
        self.updates.checking = false;
        self.updates.checked = true;
        match result {
            Ok(release) => {
                if release != self.updates.release {
                    self.updates.stage = Stage::Idle;
                }
                self.updates.release = release;
                self.updates.check_error = None;
            }
            // The release found before, if any, is still there.
            Err(e) => self.updates.check_error = Some(e),
        }
        self.publish_updates();
    }

    pub fn set_prereleases(&mut self, on: bool) {
        self.settings.prereleases = Some(on);
        self.save_settings();
        self.window().global::<ui::Rig>().set_prereleases(on);
        self.check_updates();
    }

    /// Downloads and installs the release found, on a thread: pkexec waits for the password.
    pub fn install_update(&mut self) {
        let (Some(release), Some(format)) = (self.updates.release.clone(), Format::installed())
        else {
            return;
        };
        if self.updates.stage.busy() {
            return;
        }
        self.updates.stage = Stage::Downloading(0.0);
        self.publish_updates();
        std::thread::spawn(move || {
            let mut shown = 0;
            let result = download(&release, format, &download_dir(), |done| {
                // A step per percent: the window does not need more.
                let percent = (done * 100.0) as u32;
                if percent > shown {
                    shown = percent;
                    crate::app::with_app(move |a| {
                        if let Stage::Downloading(_) = a.updates.stage {
                            a.updates.stage = Stage::Downloading(done);
                            a.publish_updates();
                        }
                    });
                }
            });
            let path = match result {
                Ok(path) => path,
                Err(e) => {
                    let text = format!("Download failed: {e:#}");
                    crate::app::with_app(move |a| a.update_finished(Stage::Failed(text)));
                    return;
                }
            };
            crate::app::with_app(|a| {
                a.updates.stage = Stage::Installing;
                a.publish_updates();
            });
            let stage = match install(&path) {
                Ok(()) => Stage::Installed,
                Err(InstallError::Denied) => {
                    Stage::Failed("Not installed: the password was not given.".into())
                }
                Err(InstallError::Failed(e)) => Stage::Failed(format!("Installation failed: {e}")),
            };
            // The package is installed or not: the download is no longer needed.
            let _ = fs::remove_dir_all(download_dir());
            crate::app::with_app(move |a| a.update_finished(stage));
        });
    }

    /// Opens the update window on a made-up release, for screenshots: `0.2.0` or
    /// `0.2.0-rc.1/downloading` (or `installing`, `installed`, `failed`); `/closed` only shows
    /// the release in the top bar and Settings.
    pub fn preview_update(&mut self, arg: &str) {
        let mut parts = arg.split('/');
        let version = parts.next().unwrap_or_default();
        let rest: Vec<&str> = parts.collect();
        let stage = rest.iter().find(|p| **p != "closed").copied().unwrap_or("");
        let Ok(version) = Version::parse(version) else {
            return;
        };
        self.updates.release = Some(Release::preview(version));
        self.updates.checked = true;
        self.updates.stage = match stage {
            "downloading" => Stage::Downloading(0.42),
            "installing" => Stage::Installing,
            "installed" => Stage::Installed,
            "failed" => Stage::Failed("Not installed: the password was not given.".into()),
            _ => Stage::Idle,
        };
        self.publish_updates();
        self.window()
            .global::<ui::Rig>()
            .set_update_open(!rest.contains(&"closed"));
    }

    fn update_finished(&mut self, stage: Stage) {
        self.updates.stage = stage;
        self.publish_updates();
    }

    /// Shows the update state: the top bar, the update window, Settings.
    pub fn publish_updates(&self) {
        let u = &self.updates;
        let running = current();
        let (stage, progress, error) = match &u.stage {
            Stage::Idle => ("idle", 0.0, String::new()),
            Stage::Downloading(p) => ("downloading", *p, String::new()),
            Stage::Installing => ("installing", 1.0, String::new()),
            Stage::Installed => ("installed", 1.0, String::new()),
            Stage::Failed(e) => ("failed", 0.0, e.clone()),
        };
        let format = Format::installed();
        let installable = u
            .release
            .as_ref()
            .is_some_and(|r| format.is_some_and(|f| r.asset(f).is_some()));
        let status = if u.checking {
            "Checking for updates…".to_string()
        } else if let Some(e) = &u.check_error {
            format!("Cannot check for updates: {e}")
        } else if let Some(r) = &u.release {
            format!("SimRig {} is available.", r.version)
        } else if u.checked {
            format!("SimRig {running} is the newest version.")
        } else {
            String::new()
        };
        let info = ui::UpdateInfo {
            available: u.release.is_some(),
            current: running.to_string().into(),
            latest: u
                .release
                .as_ref()
                .map(|r| r.version.to_string())
                .unwrap_or_default()
                .into(),
            prerelease: u.release.as_ref().is_some_and(|r| r.prerelease),
            page: u
                .release
                .as_ref()
                .map(|r| r.page.clone())
                .unwrap_or_default()
                .into(),
            stage: stage.into(),
            progress,
            error: error.into(),
            can_install: installable,
            status: status.into(),
            checking: u.checking,
        };
        self.window().global::<ui::Rig>().set_update(info);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool, sums: bool) -> String {
        let mut assets = vec![
            format!(
                r#"{{"name": "simrig_{0}-1_amd64.deb", "browser_download_url": "https://example.org/{0}.deb", "size": 10}}"#,
                tag.trim_start_matches('v').replace('-', ".")
            ),
            format!(
                r#"{{"name": "simrig-{0}-x86_64-linux.tar.gz", "browser_download_url": "https://example.org/{0}.tar.gz", "size": 10}}"#,
                tag.trim_start_matches('v')
            ),
        ];
        if sums {
            assets.push(
                r#"{"name": "SHA256SUMS", "browser_download_url": "https://example.org/sums", "size": 1}"#
                    .into(),
            );
        }
        format!(
            r#"{{"tag_name": "{tag}", "draft": false, "prerelease": {prerelease}, "html_url": "https://example.org/{tag}", "assets": [{}]}}"#,
            assets.join(",")
        )
    }

    fn list(releases: &[String]) -> String {
        format!("[{}]", releases.join(","))
    }

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn the_newest_final_release_unless_prereleases_are_wanted() {
        let json = list(&[
            release("v0.2.0-rc.1", true, true),
            release("v0.1.1", false, true),
            release("v0.1.0", false, true),
        ]);
        let found = newest(&json, &v("0.1.0"), false).unwrap().unwrap();
        assert_eq!(found.version, v("0.1.1"));
        assert!(!found.prerelease);
        assert_eq!(found.page, "https://example.org/v0.1.1");
        let found = newest(&json, &v("0.1.0"), true).unwrap().unwrap();
        assert_eq!(found.version, v("0.2.0-rc.1"));
        assert!(found.prerelease);
    }

    #[test]
    fn nothing_when_the_app_is_up_to_date() {
        let json = list(&[release("v0.1.0", false, true)]);
        assert_eq!(newest(&json, &v("0.1.0"), true).unwrap(), None);
        assert_eq!(newest(&json, &v("0.2.0"), true).unwrap(), None);
    }

    #[test]
    fn the_final_version_comes_after_its_release_candidates() {
        let json = list(&[
            release("v0.1.0", false, true),
            release("v0.1.0-rc.7", true, true),
        ]);
        let found = newest(&json, &v("0.1.0-rc.7"), false).unwrap().unwrap();
        assert_eq!(found.version, v("0.1.0"));
        // rc.10 is newer than rc.9: identifiers compare as numbers.
        let json = list(&[release("v0.1.0-rc.10", true, true)]);
        assert!(newest(&json, &v("0.1.0-rc.9"), true).unwrap().is_some());
    }

    #[test]
    fn releases_still_building_drafts_and_odd_tags_are_skipped() {
        let mut draft = release("v0.4.0", false, true);
        draft = draft.replace(r#""draft": false"#, r#""draft": true"#);
        let json = list(&[
            draft,
            release("v0.3.0", false, false),
            release("nightly", false, true),
            // Tagged as a pre-release version but not marked: still a pre-release.
            release("v0.2.0-beta.1", false, true),
        ]);
        assert_eq!(newest(&json, &v("0.1.0"), false).unwrap(), None);
    }

    #[test]
    fn the_file_for_each_installation() {
        let json = list(&[release("v0.2.0-rc.1", true, true)]);
        let found = newest(&json, &v("0.1.0"), true).unwrap().unwrap();
        assert_eq!(
            found.asset(Format::Deb).unwrap().name,
            "simrig_0.2.0.rc.1-1_amd64.deb"
        );
        assert_eq!(
            found.asset(Format::Tarball).unwrap().name,
            "simrig-0.2.0-rc.1-x86_64-linux.tar.gz"
        );
        assert_eq!(found.asset(Format::Rpm), None);
        assert!(Format::Arch.matches("simrig-0.1.0rc.7-1-x86_64.pkg.tar.zst"));
        assert!(Format::Rpm.matches("simrig-0.1.0.rc.7-1.x86_64.rpm"));
        assert_eq!(Format::parse("archlinux\n"), Some(Format::Arch));
        assert_eq!(Format::parse("snap"), None);
    }

    #[test]
    fn checksums_as_sha256sum_writes_them() {
        let sums = "AB12  simrig_0.1.0-1_amd64.deb\ncd34 *simrig-0.1.0-x86_64-linux.tar.gz\n";
        assert_eq!(
            listed_sum(sums, "simrig_0.1.0-1_amd64.deb").as_deref(),
            Some("ab12")
        );
        assert_eq!(
            listed_sum(sums, "simrig-0.1.0-x86_64-linux.tar.gz").as_deref(),
            Some("cd34")
        );
        assert_eq!(listed_sum(sums, "other"), None);
        let sums = "ef56  simrig_0.1.0~rc.7-1_amd64.deb\n";
        assert_eq!(
            listed_sum(sums, "simrig_0.1.0.rc.7-1_amd64.deb").as_deref(),
            Some("ef56")
        );
        assert_eq!(
            hex(&Sha256::digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// Asks the real GitHub (network): `cargo test -p simrig-gui -- --ignored`.
    #[test]
    #[ignore]
    fn github_lists_the_releases() {
        let text = agent()
            .get(RELEASES)
            .call()
            .unwrap()
            .body_mut()
            .read_to_string()
            .unwrap();
        let found = newest(&text, &v("0.0.0"), true).unwrap().unwrap();
        for format in [Format::Deb, Format::Rpm, Format::Arch, Format::Tarball] {
            assert!(found.asset(format).is_some(), "{format:?} in {found:?}");
        }
    }

    #[test]
    fn github_answers_other_than_a_list_are_errors() {
        assert!(
            newest(
                r#"{"message": "API rate limit exceeded"}"#,
                &v("0.1.0"),
                true
            )
            .is_err()
        );
    }
}
