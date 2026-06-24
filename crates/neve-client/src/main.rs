use std::{
    io::{self, BufRead},
    thread,
};

use clap::Parser;
use neve_protocol::{
    AUTH_TOKEN_KEY, AuthenticateRequest, AuthenticateResponse, ChatRequest, ChatResponse,
    auth_service_client::AuthServiceClient, chat_service_client::ChatServiceClient,
};
use tokio::{select, sync::mpsc};
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tonic::{Request, metadata::AsciiMetadataValue, transport::Channel};
use tracing::{debug, error, info};

#[derive(Parser)]
struct ClientArgs {
    #[arg(long)]
    username: String,

    #[arg(long)]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .without_time()
        .init();

    let args = ClientArgs::parse();

    let AuthenticateResponse { auth_token } =
        AuthServiceClient::connect(format!("http://[::]:{}", args.port))
            .await?
            .authenticate(tonic::Request::new(AuthenticateRequest {
                username: args.username,
            }))
            .await?
            .into_inner();

    let auth_token = auth_token.parse::<AsciiMetadataValue>()?;

    let add_auth_token = move |mut request: Request<()>| {
        request
            .metadata_mut()
            .insert(AUTH_TOKEN_KEY, auth_token.clone());

        Ok(request)
    };

    let channel = Channel::from_shared(format!("http://[::]:{}", args.port))?
        .connect()
        .await?;

    let mut client = ChatServiceClient::with_interceptor(channel, add_auth_token);

    let (messages_tx, messages_rx) = mpsc::channel(128);

    let requests = ReceiverStream::new(messages_rx).map(|message| ChatRequest { message });

    let mut responses = client.chat(requests).await?.into_inner();

    let (input_tx, mut input_rx) = mpsc::channel(128);
    thread::spawn(move || {
        let mut stdin = io::stdin().lock();

        loop {
            let mut input = String::new();

            match stdin.read_line(&mut input) {
                Ok(0) => break,
                Ok(_) => {
                    // Strip newline.
                    input.pop();

                    if input_tx.blocking_send(input).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    error!(?err, "Error reading stdin");
                }
            }
        }
    });

    loop {
        select! {
            response = responses.next() => match response {
                Some(response) => {
                    if let Ok(ChatResponse { message, from }) = response {
                        debug!(?message, ?from);
                    }
                }
                None => break,
            },
            input = input_rx.recv() => match input {
                Some(input) => {
                    if messages_tx.send(input).await.is_err() {
                        info!("Receiving channel closed, exiting");
                        break;
                    }
                }
                None => break,
            }
        }
    }

    info!("Connection closed");

    Ok(())
}
