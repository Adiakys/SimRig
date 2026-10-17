//! `simrig-plugin-logitech`: the adapter of the Logitech plugin (runs with the app).

fn main() -> std::process::ExitCode {
    simrig_plugin_sdk::run(simrig_plugin_logitech::adapter::LogitechAdapter::from_args())
}
