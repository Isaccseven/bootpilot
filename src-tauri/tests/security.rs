use serde_json::{Value, json};

#[test]
fn desktop_capability_is_local_and_main_only() {
    let capability: Value =
        serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
    assert_eq!(capability["windows"], json!(["main"]));
    assert_eq!(capability["permissions"], json!(["bootpilot-operations"]));
    assert!(capability.get("remote").is_none());
    let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    assert_eq!(config["app"]["security"]["capabilities"], json!(["main"]));
    let csp = config["app"]["security"]["csp"].as_str().unwrap();
    assert!(csp.contains("script-src 'self'"));
    assert!(csp.contains("connect-src ipc: http://ipc.localhost"));
    for unsafe_source in ["https:", "unsafe-eval", "unsafe-inline", "*"] {
        assert!(!csp.contains(unsafe_source));
    }
    let permissions = include_str!("../permissions/bootpilot.toml");
    for broad_plugin in ["shell:", "fs:", "core:default"] {
        assert!(!permissions.contains(broad_plugin));
    }
}
