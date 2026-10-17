//! `simrig-generic-usb-service <hidrawN | usb:NAME> <devices dir>`: the driver's service,
//! started by udev for a set-up device (its `hidraw` node, or for a device without one its USB
//! device, as named in `/sys/bus/usb/devices`) and stopped when it is unplugged.

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [hidraw, dir] = args.as_slice() else {
        eprintln!("usage: simrig-generic-usb-service <hidrawN | usb:NAME> <devices dir>");
        return ExitCode::from(2);
    };
    let dir = Path::new(dir);
    let result = match hidraw.strip_prefix("usb:") {
        Some(name) => simrig_plugin_generic_usb::service::run_usb(name, dir),
        None => simrig_plugin_generic_usb::service::run(hidraw.trim_start_matches("/dev/"), dir),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
