#![forbid(unsafe_code)]

use aise_service::observability::ObservabilityBootstrap;
use aise_service::web::{self, WebConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let observability = ObservabilityBootstrap::initialize();
    let service_result = match WebConfig::load() {
        Ok(config) => web::run(config, observability.config()).await,
        Err(error) => Err(error),
    };
    observability.shutdown().await?;
    service_result
}
