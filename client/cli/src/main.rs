use clap::Parser;
use tonic::Request;
use tracing::info;

use neve_proto::server::v1::{
    AuthenticateRequest, AuthenticateResponse, RegisterRequest,
    auth_service_client::AuthServiceClient,
};

#[derive(Parser)]
struct ClientArgs {
    #[arg(long)]
    username: String,

    #[arg(long)]
    password: String,

    #[arg(long)]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = ClientArgs::parse();

    let endpoint = format!("http://[::]:{}", args.port);

    let mut auth_client = AuthServiceClient::connect(endpoint.clone()).await?;

    _ = auth_client
        .register(Request::new(RegisterRequest {
            username: args.username.clone(),
            password: args.password.clone(),
        }))
        .await;

    let AuthenticateResponse { auth_token } = auth_client
        .authenticate(Request::new(AuthenticateRequest {
            username: args.username,
            password: args.password,
        }))
        .await?
        .into_inner();

    info!(?auth_token, "Authenticated");

    Ok(())
}
