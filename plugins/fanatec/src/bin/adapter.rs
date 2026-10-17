//! `simrig-plugin-fanatec`: the adapter of the Fanatec plugin (runs with the app).

fn main() -> std::process::ExitCode {
    simrig_plugin_sdk::run(simrig_plugin_fanatec::adapter::FanatecAdapter::from_args())
}
