//! `simrig-plugin-simagic`: the adapter of the Simagic plugin (runs with the app).

fn main() -> std::process::ExitCode {
    simrig_plugin_sdk::run(simrig_plugin_simagic::adapter::SimagicAdapter::from_args())
}
