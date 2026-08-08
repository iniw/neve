use std::net::{IpAddr, Ipv6Addr, SocketAddr};

use clap::Parser;
use sqlx::PgPool;
use tokio::signal::unix::{SignalKind, signal};
use tonic::transport::Server;
use tracing::info;

mod auth;
mod chat;
mod db;

use auth::AuthServer;
use chat::ChatServer;

#[derive(Parser)]
struct ServerArgs {
    #[arg(long)]
    port: u16,

    #[arg(long, env = "DATABASE_URL")]
    database_url: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = ServerArgs::parse();

    let addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), args.port);
    info!(?addr, "Server starting");

    let pool = PgPool::connect(&args.database_url).await?;
    info!(?pool, "Connected to database");

    sqlx::migrate!().run(&pool).await?;
    info!(?pool, "Ran database migrations");

    let auth_server = AuthServer::new(pool.clone());
    let chat_server = ChatServer::new(pool);

    let serve = Server::builder()
        .add_service(auth_server.auth_interceptor(chat_server.service()))
        .add_service(auth_server.service())
        .serve(addr);

    let mut sigterm = signal(SignalKind::terminate())?;
    let sigterm = sigterm.recv();

    let mut sigint = signal(SignalKind::interrupt())?;
    let sigint = sigint.recv();

    tokio::select! {
        result = serve => result?,

        _ = sigterm => {
            info!("Stopped server after SIGTERM");
        },

        _ = sigint => {
            info!("Stopped server after SIGINT");
        },
    }

    Ok(())
}
