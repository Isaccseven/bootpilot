pub mod tools;
use boot_service::BootService;
use rmcp::{ServiceExt, transport::stdio};

pub async fn serve(service: BootService) -> Result<(), Box<dyn std::error::Error>> {
    let running = tools::BootMcp::new(service).serve(stdio()).await?;
    running.waiting().await?;
    Ok(())
}
