//! `simrig-generic-usb-service <hidrawN> <devices dir>`: the driver's service, started by udev
//! for a set-up device and stopped when it is unplugged.

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [hidraw, dir] = args.as_slice() else {
        eprintln!("usage: simrig-generic-usb-service <hidrawN> <devices dir>");
        return ExitCode::from(2);
    };
    let hidraw = hidraw.trim_start_matches("/dev/");
    match simrig_plugin_generic_usb::service::run(hidraw, Path::new(dir)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
