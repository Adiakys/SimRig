use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::DriverError;

const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// Where and with what a hook runs. Hooks get a clean environment:
/// `SIMRIG_PLUGIN`, `SIMRIG_DRIVER_DIR`, `SIMRIG_DRIVER_DATA` (the driver's own data directory)
/// and one `SIMRIG_ARG_<NAME>` per argument of a privileged operation.
pub struct HookContext {
    pub plugin: String,
    pub driver_dir: PathBuf,
    pub data_dir: PathBuf,
    pub args: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookOutput {
    pub stdout: String,
    pub stderr: String,
}

/// Runs a hook (a path relative to the driver directory) and waits at most `timeout`.
pub fn run_hook(
    ctx: &HookContext,
    hook: &str,
    timeout: Duration,
) -> Result<HookOutput, DriverError> {
    let driver_dir = ctx.driver_dir.canonicalize()?;
    let program = driver_dir.join(hook).canonicalize()?;
    if !program.starts_with(&driver_dir) {
        return Err(DriverError::Invalid {
            path: program,
            message: "hook outside the driver directory".into(),
        });
    }
    let mut command = Command::new(&program);
    command
        .current_dir(&driver_dir)
        .env_clear()
        .env("PATH", PATH)
        .env("SIMRIG_PLUGIN", &ctx.plugin)
        .env("SIMRIG_DRIVER_DIR", &driver_dir)
        .env("SIMRIG_DRIVER_DATA", &ctx.data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in &ctx.args {
        command.env(
            format!("SIMRIG_ARG_{}", name.to_ascii_uppercase().replace('-', "_")),
            value,
        );
    }
    let mut child = command.spawn()?;
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_string(&mut text);
            }
            text
        })
    };
    let stdout = read(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = read(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(DriverError::HookTimeout(hook.into()));
        }
        thread::sleep(Duration::from_millis(20));
    };
    let output = HookOutput {
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    };
    if !status.success() {
        return Err(DriverError::HookFailed {
            hook: hook.into(),
            status: status.to_string(),
            stderr: output.stderr.trim().to_string(),
        });
    }
    Ok(output)
}
