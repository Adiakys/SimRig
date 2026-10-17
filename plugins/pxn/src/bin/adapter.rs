//! `simrig-plugin-pxn`: the adapter of the PXN plugin (runs with the app).

fn main() -> std::process::ExitCode {
    simrig_plugin_sdk::run(simrig_plugin_pxn::adapter::PxnAdapter::from_args())
}
