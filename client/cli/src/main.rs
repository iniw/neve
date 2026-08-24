use clap::Parser;
use tonic::Request;
use tracing::{info, level_filters::LevelFilter};

use neve_proto::server::v1::{
    AuthenticateRequest, AuthenticateResponse, RegisterRequest,
    auth_service_client::AuthServiceClient,
};
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan, util::SubscriberInitExt};

#[derive(Parser)]
struct ClientArgs {
    /// The port to connect to the server on.
    #[arg(long)]
    port: u16,

    /// The filter to use for the client's tracing logs.
    ///
    /// See <https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives> for
    /// the syntax definition.
    #[arg(long)]
    tracing_filter: Option<String>,

    /// The username to use to authenticate this client.
    #[arg(long)]
    username: String,

    /// The password to use to authenticate this client.
    #[arg(long)]
    password: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = ClientArgs::parse();

    init_tracing(args.tracing_filter.as_deref());

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

fn init_tracing(filter: Option<&str>) {
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .parse_lossy(filter.unwrap_or_default());

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .finish()
        .init()
}
