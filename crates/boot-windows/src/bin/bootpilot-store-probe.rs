//! Development-only isolated protected-store persistence gate. Never a boot target test.
#[cfg(debug_assertions)]
fn main() {
    use std::io::Write;
    let args: Vec<_> = std::env::args().collect();
    let id = if args.len() == 3 && matches!(args[1].as_str(), "--run-id" | "--verify-run-id") {
        uuid::Uuid::parse_str(&args[2]).ok()
    } else {
        None
    };
    let Some(id) = id else { std::process::exit(2) };
    let verify = args[1] == "--verify-run-id";
    let prefix = if verify {
        "BootPilot-store-verify"
    } else {
        "BootPilot-store-probe"
    };
    let path = std::env::temp_dir().join(format!("{prefix}-{id}.json"));
    let Ok(mut file) = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    else {
        std::process::exit(3)
    };
    let result = if verify {
        boot_windows::store_probe::verify(id)
    } else {
        boot_windows::store_probe::run(id)
    };
    let bytes = serde_json::to_vec_pretty(&result).unwrap();
    if file
        .write_all(&bytes)
        .and_then(|_| file.sync_all())
        .is_err()
    {
        std::process::exit(4)
    }
    if result.is_err() {
        std::process::exit(1)
    }
}
#[cfg(not(debug_assertions))]
fn main() {
    eprintln!("Development-only probe is unavailable in release builds.");
    std::process::exit(2);
}
