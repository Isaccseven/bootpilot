mod commands;
use std::sync::Arc;

pub fn run() {
    tauri::Builder::default()
        .manage(Arc::new(boot_service::BootService::new()))
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::refresh,
            commands::diagnose,
            commands::prepare,
            commands::apply,
            commands::list_backups,
            commands::get_integration_status,
            commands::register_codex,
        ])
        .run(tauri::generate_context!())
        .expect("BootPilot could not start");
}
