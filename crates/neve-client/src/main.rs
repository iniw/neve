use clap::Parser;
use neve_protocol::{ChatRequest, ShareInfoRequest, neve_service_client::NeveServiceClient};
use tokio::{
    io::{AsyncBufReadExt, BufReader, stdin},
    sync::mpsc,
};
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tracing::{debug, error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .without_time()
        .init();

    let args = ClientArgs::parse();

    let mut client = NeveServiceClient::connect(format!("http://[::]:{}", args.port)).await?;

    info!("Connected to server");

    _ = client
        .share_info(tonic::Request::new(ShareInfoRequest { name: args.name }))
        .await?;

    let (tx, rx) = mpsc::channel(128);

    tokio::spawn(async move {
        let mut stdin = BufReader::new(stdin());
        loop {
            let mut buf = String::new();
            match stdin.read_line(&mut buf).await {
                Ok(0) => break,
                Ok(_) => {
                    // Strip newline
                    buf.pop();

                    if tx.send(buf).await.is_err() {
                        info!("Receiving channel closed, exiting");
                        return;
                    }
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
            debug!(?response);
        }
    }

    Ok(())
}

#[derive(Parser)]
struct ClientArgs {
    #[arg(long)]
    name: String,

    #[arg(long)]
    port: u16,
}
