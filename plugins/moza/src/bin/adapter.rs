//! `simrig-plugin-moza`: the adapter of the Moza plugin (runs with the app).

fn main() -> std::process::ExitCode {
    simrig_plugin_sdk::run(simrig_plugin_moza::adapter::MozaAdapter::from_args())
}
