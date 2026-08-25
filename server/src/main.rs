use std::{
    net::{IpAddr, Ipv6Addr, SocketAddr},
    path::PathBuf,
};

use clap::Parser;
use sqlx::PgPool;
use tokio::signal::unix::{SignalKind, signal};
use tonic::transport::{Identity, Server, ServerTlsConfig};
use tonic_web::GrpcWebLayer;
use tower_http::cors::CorsLayer;
use tracing::{info, level_filters::LevelFilter};

mod auth;
mod chat;
mod error;
mod message;

use auth::AuthServer;
use chat::ChatServer;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan, util::SubscriberInitExt};

use crate::message::MessageServer;

#[derive(Parser)]
struct ServerArgs {
    /// The port in which the server will listen on.
    #[arg(long, env = "SERVER_PORT")]
    port: u16,

    /// The filter to use for the server's tracing events.
    ///
    /// See <https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives> for
    /// the syntax definition.
    #[arg(long, env = "SERVER_TRACING_FILTER")]
    tracing_filter: Option<String>,

    /// The connection string that the server will use to connect to the database.
    ///
    /// See <https://www.postgresql.org/docs/current/libpq-connect.html#LIBPQ-CONNSTRING> for the syntax definition.
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    /// The PEM certificate chain that the server will use for TLS.
    #[arg(long, env = "SERVER_TLS_CERTIFICATE")]
    tls_certificate: PathBuf,

    /// The PEM private key that the server will use for TLS.
    #[arg(long, env = "SERVER_TLS_PRIVATE_KEY")]
    tls_private_key: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = ServerArgs::parse();

    init_tracing(args.tracing_filter.as_deref());

    let addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), args.port);
    info!(?addr, "Server starting");

    let pool = PgPool::connect(&args.database_url).await?;
    info!(?pool, "Connected to database");

    sqlx::migrate!().run(&pool).await?;
    info!("Ran database migrations");

    let auth_server = AuthServer::new(pool.clone());
    let chat_server = ChatServer::new(pool.clone());
    let message_server = MessageServer::new(pool);

    let server = Server::builder()
        .tls_config(ServerTlsConfig::new().identity(Identity::from_pem(
            tokio::fs::read(args.tls_certificate).await?,
            tokio::fs::read(args.tls_private_key).await?,
        )))?
        .layer(CorsLayer::permissive().allow_credentials(false))
        .layer(GrpcWebLayer::new())
        .add_service(auth_server.interceptor(chat_server.service()))
        .add_service(auth_server.interceptor(message_server.service()))
        .add_service(auth_server.service());

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    tokio::select! {
        result = server.serve(addr) => result?,

        _ = sigterm.recv() => {
            info!("Stopped server after SIGTERM");
        },

        _ = sigint.recv() => {
            info!("Stopped server after SIGINT");
        },
    }

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
