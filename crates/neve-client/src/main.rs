use anyhow::bail;
use neve_protocol::{SERVER_PORT, ShareInfoRequest, neve_service_client::NeveServiceClient};
use std::env::args;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let Some(name) = args().nth(1) else {
        bail!("Usage: neve-client [name]");
    };

    let mut client = NeveServiceClient::connect(format!("http://[::1]:{SERVER_PORT}")).await?;

    info!("Connected to server");

    let request = tonic::Request::new(ShareInfoRequest { name });

    _ = client.share_info(request).await?;

    Ok(())
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().compact())
        .init();
}
