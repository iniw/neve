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
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan, util::SubscriberInitExt};

use crate::{
    auth::{AuthServer, PasetoKey},
    chat::ChatServer,
    message::MessageServer,
};

mod auth;
mod chat;
mod error;
mod message;

#[derive(Parser)]
struct ServerArgs {
    /// The port to listen on.
    #[arg(long, env, default_value = "5547")]
    port: u16,

    /// The connection string to use for the postgres database connection.
    ///
    /// See <https://www.postgresql.org/docs/current/libpq-connect.html#LIBPQ-CONNSTRING> for the syntax definition.
    #[arg(long, env, default_value = "postgres://localhost/neve")]
    database_url: String,

    /// The path to a PEM certificate chain to use for TLS.
    #[arg(long, env, default_value = ".cert/localhost.pem")]
    tls_certificate: PathBuf,

    /// The path to a PEM private key to use for TLS.
    #[arg(long, env, default_value = ".cert/localhost.key.pem")]
    tls_private_key: PathBuf,

    /// The key to use for the paseto symmetric key pair.
    ///
    /// The value must be encoded in hexadecimal and decode down to a 32 byte array.
    #[arg(
        long,
        env,
        default_value = "6F73706172616C616D6173646F7375636573736F2D6F706173736F646F6C7569",
        value_parser = |value: &str| PasetoKey::try_from(value)
    )]
    paseto_key: PasetoKey,

    /// The filter to use for tracing events.
    ///
    /// See <https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives> for
    /// the syntax definition.
    #[arg(long, env, default_value = "info")]
    tracing_filter: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = ServerArgs::parse();

    init_tracing(&args.tracing_filter);

    let addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), args.port);
    info!(?addr, "Server starting");

    let pool = PgPool::connect(&args.database_url).await?;
    info!(?pool, "Connected to database");

    sqlx::migrate!().run(&pool).await?;
    info!("Ran database migrations");

    let auth_server = AuthServer::new(pool.clone(), args.paseto_key);
    let chat_server = ChatServer::new(pool.clone());
    let message_server = MessageServer::new(pool).await?;

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

fn init_tracing(filter: &str) {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::builder().parse_lossy(filter))
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .finish()
        .init()
}
