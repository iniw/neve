use std::pin::Pin;

use clap::Parser;
use sqlx::PgPool;
use tokio::sync::broadcast;
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};
use tonic::{Request, Response, Status, transport::Server};
use tracing::{Instrument, debug, info};

use neve_proto::server::v1::{
    ChatRequest, ChatResponse,
    chat_service_server::{ChatService, ChatServiceServer},
};

mod auth;
mod db;

use crate::auth::{AuthInfo, AuthServer};

#[derive(Parser)]
struct ServerArgs {
    #[arg(long)]
    port: u16,

    #[arg(long, env = "DATABASE_URL")]
    database_url: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .without_time()
        .init();

    let args = ServerArgs::parse();

    let addr = format!("[::]:{}", args.port).parse()?;
    info!(?addr, "Server starting");

    let pool = PgPool::connect(&args.database_url).await?;
    sqlx::migrate!().run(&pool).await?;
    info!(?pool, "Connected to database; migrations executed");

    let auth_server = AuthServer::new(pool.clone());
    let chat_server = ChatServer::new(pool);

    Server::builder()
        .add_service(auth_server.auth_interceptor(ChatServiceServer::new(chat_server)))
        .add_service(auth_server.service())
        .serve(addr)
        .await?;

    Ok(())
}

struct ChatServer {
    db: PgPool,

    messages_tx: broadcast::Sender<ChatResponse>,
    messages_rx: broadcast::Receiver<ChatResponse>,
}

impl ChatServer {
    fn new(db: PgPool) -> Self {
        let (messages_tx, messages_rx) = broadcast::channel(128);

        Self {
            db,
            messages_tx,
            messages_rx,
        }
    }
}

#[tonic::async_trait]
impl ChatService for ChatServer {
    type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatResponse, Status>> + Send + 'static>>;

    #[tracing::instrument(skip_all, err)]
    async fn chat(
        &self,
        request: Request<tonic::Streaming<ChatRequest>>,
    ) -> Result<Response<Self::ChatStream>, Status> {
        let auth_info = request
            .extensions()
            .get::<AuthInfo>()
            .copied()
            .ok_or(Status::unauthenticated("No authentication info"))?;

        debug!(?auth_info.account_id, "New connection");

        let account = sqlx::query!(
            r#"
                SELECT username
                FROM account
                WHERE id = $1
                LIMIT 1
            "#,
            auth_info.account_id
        )
        .fetch_optional(&self.db)
        .await
        .map_err(db::error())?
        .ok_or(Status::not_found("Account not found"))?;

        let messages_tx = self.messages_tx.clone();
        tokio::spawn(
            async move {
                let mut request_stream = request.into_inner();

                while let Some(request) = request_stream.next().await {
                    let Ok(ChatRequest { message }) = request else {
                        continue;
                    };

                    let from = account.username.clone();

                    debug!(?message, ?from);

                    if messages_tx.send(ChatResponse { message, from }).is_err() {
                        return;
                    }
                }
            }
            .in_current_span(),
        );

        let responses = BroadcastStream::new(self.messages_rx.resubscribe())
            .map(|message| message.map_err(|_| Status::internal("Server closed")));

        Ok(Response::new(Box::pin(responses)))
    }
}
