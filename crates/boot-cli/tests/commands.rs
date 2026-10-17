use std::process::Command;

#[test]
fn cached_commands_need_no_elevation_and_report_no_capture_honestly() {
    let root = std::env::temp_dir().join(format!(
        "bootpilot-cli-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_boot-cli"))
            .args(args)
            .env("LOCALAPPDATA", &root)
            .output()
            .unwrap()
    };
    let status = run(&["status", "--json"]);
    assert!(status.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&status.stdout).unwrap(),
        serde_json::Value::Null
    );
    assert!(status.stderr.is_empty());
    let human = run(&["status"]);
    assert!(human.status.success());
    assert!(
        String::from_utf8(human.stdout)
            .unwrap()
            .contains("Noch keine Erfassung")
    );
    let diagnose = run(&["diagnose", "--json"]);
    assert_eq!(diagnose.status.code(), Some(3));
    assert!(diagnose.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&diagnose.stderr).unwrap()["code"],
        "incomplete_snapshot"
    );
    let invalid = run(&["status", "--write"]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
    assert!(
        !root.exists(),
        "read commands do not even create a cache directory"
    );
}
