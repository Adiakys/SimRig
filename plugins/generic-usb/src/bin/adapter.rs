//! `simrig-plugin-generic-usb`: the adapter of the generic USB plugin (runs with the app).

fn main() -> std::process::ExitCode {
    simrig_plugin_sdk::run(simrig_plugin_generic_usb::adapter::GenericAdapter::from_env())
}
