#[tokio::main]
async fn main() {
    if let Err(error) = boot_mcp::serve(boot_service::BootService::new()).await {
        eprintln!("BootPilot MCP: {error}");
        std::process::exit(1);
    }
}
