#![cfg_attr(windows, windows_subsystem = "windows")]
fn main() {
    if boot_windows::broker::run_helper().is_err() {
        std::process::exit(1);
    }
}
