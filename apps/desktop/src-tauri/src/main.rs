#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--nulis-plugin-worker") {
        if nulis_plugin_host::run_worker_stdio().is_err() {
            std::process::exit(1);
        }
        return;
    }
    nulis_desktop_lib::run();
}
