fn main() {
    if let Err(error) = nulis_plugin_host::run_worker_stdio() {
        eprintln!("plugin worker transport failed: {error}");
        std::process::exit(1);
    }
}
