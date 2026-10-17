fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_status",
            "refresh",
            "diagnose",
            "prepare",
            "apply",
            "list_backups",
            "get_integration_status",
            "register_codex",
        ]),
    ))
    .expect("Tauri app manifest and permissions must be valid");
}
