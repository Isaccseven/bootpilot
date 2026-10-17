use boot_service::integration::register_codex_at;
use fs2::FileExt;
use std::{fs, path::PathBuf};

fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let mcp = dir.path().join("boot-mcp.exe");
    fs::copy(std::env::current_exe().unwrap(), &mcp).unwrap();
    (dir, config, mcp)
}
#[test]
fn registration_preserves_rea_and_other_servers() {
    let (_dir, config, mcp) = setup();
    let original = "# User settings\nmodel = 'test' # chosen\n\n[mcp_servers.rea]\ncommand = 'C:\\REA\\rea.exe' # keep\nargs = ['--stdio']\n\n[mcp_servers.other]\nurl = 'https://example.invalid'\n";
    fs::write(&config, original).unwrap();
    let result = register_codex_at(&config, &mcp).unwrap();
    let edited = fs::read_to_string(&config).unwrap();
    assert!(edited.starts_with(original));
    let doc = edited.parse::<toml_edit::DocumentMut>().unwrap();
    assert_eq!(
        doc["mcp_servers"]["bootpilot"]["command"].as_str(),
        mcp.to_str()
    );
    assert_eq!(fs::read_to_string(result.backup_path).unwrap(), original);
    assert!(result.restart_required);
}
#[test]
fn registration_is_idempotent() {
    let (_dir, config, mcp) = setup();
    fs::write(&config, "# keep\n").unwrap();
    register_codex_at(&config, &mcp).unwrap();
    let first = fs::read(&config).unwrap();
    register_codex_at(&config, &mcp).unwrap();
    assert_eq!(first, fs::read(&config).unwrap());
}
#[test]
fn invalid_mcp_path_does_not_modify_config() {
    let (_dir, config, _mcp) = setup();
    fs::write(&config, "# keep\n").unwrap();
    for invalid in [
        PathBuf::from("relative.exe"),
        config.with_file_name("missing.exe"),
        config.clone(),
    ] {
        assert!(register_codex_at(&config, &invalid).is_err());
        assert_eq!(fs::read_to_string(&config).unwrap(), "# keep\n");
    }
}
#[test]
fn malformed_config_is_not_replaced() {
    let (_dir, config, mcp) = setup();
    fs::write(&config, "[broken").unwrap();
    assert!(register_codex_at(&config, &mcp).is_err());
    assert_eq!(fs::read_to_string(config).unwrap(), "[broken");
}
#[test]
fn concurrent_config_writer_is_rejected() {
    let (_dir, config, mcp) = setup();
    fs::write(&config, "# preserve\n").unwrap();
    let held = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&config)
        .unwrap();
    held.try_lock_exclusive().unwrap();
    assert!(register_codex_at(&config, &mcp).is_err());
    drop(held);
    assert_eq!(fs::read_to_string(config).unwrap(), "# preserve\n");
}
#[test]
fn file_existence_does_not_verify_mcp_connection() {
    let (_dir, config, mcp) = setup();
    // An existing MZ file is deliberately not a functioning MCP server.
    fs::write(&mcp, b"MZnot-an-executable").unwrap();
    let mut doc = toml_edit::DocumentMut::new();
    doc["mcp_servers"]["bootpilot"]["command"] = toml_edit::value(mcp.to_str().unwrap());
    doc["mcp_servers"]["bootpilot"]["args"] = toml_edit::value(toml_edit::Array::new());
    fs::write(&config, doc.to_string()).unwrap();
    let status = boot_service::integration::integration_status_at(&config, &mcp).unwrap();
    assert!(status.configured);
    assert!(!status.connection_verified);
}
#[test]
fn invalid_executable_does_not_modify_config() {
    let (_dir, config, mcp) = setup();
    fs::write(&config, "# unchanged\n").unwrap();
    fs::write(&mcp, b"MZnot-an-executable").unwrap();
    assert!(register_codex_at(&config, &mcp).is_err());
    assert_eq!(fs::read_to_string(config).unwrap(), "# unchanged\n");
}
#[test]
fn registration_preserves_comments_and_enables_its_own_server() {
    let (_dir, config, mcp) = setup();
    fs::write(&config, "# preferences\n[mcp_servers.bootpilot] # own server\ncommand = 'old.exe' # local command\nargs = ['old'] # local arguments\nenabled = false # explicit toggle\n").unwrap();
    register_codex_at(&config, &mcp).unwrap();
    let text = fs::read_to_string(config).unwrap();
    for comment in [
        "# preferences",
        "# own server",
        "# local command",
        "# local arguments",
        "# explicit toggle",
    ] {
        assert!(text.contains(comment), "{comment}");
    }
    let doc = text.parse::<toml_edit::DocumentMut>().unwrap();
    assert_eq!(
        doc["mcp_servers"]["bootpilot"]["enabled"].as_bool(),
        Some(true)
    );
}
#[test]
fn invalid_path_does_not_create_a_new_config_directory() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("not-created").join("config.toml");
    assert!(register_codex_at(&config, &dir.path().join("missing.exe")).is_err());
    assert!(!config.parent().unwrap().exists());
}
#[test]
#[cfg(windows)]
fn unrelated_editor_write_handle_blocks_registration_without_advisory_lock() {
    let (_dir, config, mcp) = setup();
    fs::write(&config, "# external editor\n").unwrap();
    let held = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&config)
        .unwrap();
    assert_eq!(
        register_codex_at(&config, &mcp).unwrap_err().code,
        boot_core::ErrorCode::Conflict
    );
    drop(held);
    assert_eq!(fs::read_to_string(config).unwrap(), "# external editor\n");
}
#[test]
fn windows_line_endings_and_comments_of_other_tables_are_preserved() {
    let (_dir, config, mcp) = setup();
    let original = "# windows config\r\nmodel = 'unchanged'\r\n\r\n[mcp_servers.rea]\r\ncommand = 'REA.exe' # preserve\r\nargs = []\r\n";
    fs::write(&config, original).unwrap();
    register_codex_at(&config, &mcp).unwrap();
    assert!(fs::read_to_string(config).unwrap().starts_with(original));
}
