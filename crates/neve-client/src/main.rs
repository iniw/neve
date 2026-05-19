use anyhow::bail;
use neve_protocol::{
    ChatRequest, SERVER_PORT, ShareInfoRequest, neve_service_client::NeveServiceClient,
};
use std::env::args;
use tokio::{
    io::{AsyncBufReadExt, BufReader, stdin},
    sync::mpsc,
};
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tracing::{debug, error, info};
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

    let (tx, rx) = mpsc::channel(128);

    tokio::spawn(async move {
        let mut stdin = BufReader::new(stdin());
        loop {
            let mut buf = String::new();
            match stdin.read_line(&mut buf).await {
                Ok(0) => break,
                Ok(_) => {
                    buf.pop();
                    _ = tx.send(buf).await;
                }
                Err(err) => {
                    error!(?err, "Error reading stdin");
                }
            }
        }
    });

    let requests = ReceiverStream::new(rx).map(|message| ChatRequest { message });

    let mut responses = client.chat(requests).await?.into_inner();

    while let Some(response) = responses.next().await {
        if let Ok(response) = response {
            debug!(?response, "Got a response");
        }
    }

    Ok(())
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().compact())
        .init();
}
