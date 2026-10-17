use boot_windows::{BrokerClient, BrokerRequest, BrokerResponse, collect_snapshot};
fn main() {
    let result = match std::env::args().nth(1).as_deref() {
        Some("--local") => collect_snapshot().map(|s| BrokerResponse::Snapshot(Box::new(s))),
        Some("--broker") => BrokerClient::new().request(BrokerRequest::Refresh),
        _ => {
            eprintln!("Usage: bootpilot-read-probe --local|--broker");
            std::process::exit(2);
        }
    };
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
    if result.is_err() {
        std::process::exit(1);
    }
}
