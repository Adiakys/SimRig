//! `simrig-plugin-pidff`: the adapter of the standard force feedback plugin (runs with the app).

fn main() -> std::process::ExitCode {
    simrig_plugin_sdk::run(simrig_plugin_pidff::adapter::PidffAdapter::from_args())
}
