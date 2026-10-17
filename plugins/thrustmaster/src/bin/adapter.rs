//! `simrig-plugin-thrustmaster`: the adapter of the Thrustmaster plugin (runs with the app).

fn main() -> std::process::ExitCode {
    simrig_plugin_sdk::run(simrig_plugin_thrustmaster::adapter::ThrustmasterAdapter::from_args())
}
