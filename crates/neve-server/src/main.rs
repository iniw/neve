use clap::Parser;
use neve_protocol::{
    AUTH_TOKEN_HEADER, AuthenticateRequest, AuthenticateResponse, ChatRequest, ChatResponse,
    auth_service_server::{AuthService, AuthServiceServer},
    chat_service_server::{ChatService, ChatServiceServer},
};
use std::{
    collections::HashMap,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};
use tokio::sync::{RwLock, broadcast};
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};
use tonic::{
    Request, Response, Status,
    body::Body,
    codegen::http::{HeaderValue, Request as HttpRequest},
    transport::Server,
};
use tonic_middleware::{InterceptorFor, RequestInterceptor};
use tracing::{Instrument, debug, info};

#[derive(Parser)]
struct ServerArgs {
    #[arg(long)]
    port: u16,
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

    let auth_db = AuthDb::default();

    let auth_server = AuthServer::new(auth_db.clone());
    let chat_server = ChatServer::new();

    let auth_interceptor = AuthInterceptor { auth_db };

    Server::builder()
        .add_service(AuthServiceServer::new(auth_server))
        .add_service(InterceptorFor::new(
            ChatServiceServer::new(chat_server),
            auth_interceptor,
        ))
        .serve(addr)
        .await?;

    Ok(())
}

struct ChatServer {
    messages_tx: broadcast::Sender<ChatResponse>,
    messages_rx: broadcast::Receiver<ChatResponse>,
}

impl ChatServer {
    fn new() -> Self {
        let (messages_tx, messages_rx) = broadcast::channel(128);

        Self {
            messages_tx,
            messages_rx,
        }
    }
}

#[tonic::async_trait]
impl ChatService for ChatServer {
    type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatResponse, Status>> + Send + 'static>>;

    #[tracing::instrument(err, skip_all)]
    async fn chat(
        &self,
        request: Request<tonic::Streaming<ChatRequest>>,
    ) -> Result<Response<Self::ChatStream>, Status> {
        let Some(auth_info) = request.extensions().get::<AuthInfo>().cloned() else {
            return Err(Status::unauthenticated("No authentication info"));
        };

        debug!(?auth_info.username, "New connection");

        let messages_tx = self.messages_tx.clone();
        tokio::spawn(
            async move {
                let mut request_stream = request.into_inner();

                while let Some(request) = request_stream.next().await {
                    let Ok(ChatRequest { message }) = request else {
                        continue;
                    };

                    let from = auth_info.username.clone();

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

struct AuthServer {
    auth_db: AuthDb,

    // TODO: Make this an asymmetric key issuer.
    auth_counter: AtomicU32,
}

impl AuthServer {
    fn new(auth_db: AuthDb) -> Self {
        Self {
            auth_db,
            auth_counter: AtomicU32::new(0),
        }
    }
}

#[tonic::async_trait]
impl AuthService for AuthServer {
    #[tracing::instrument(err, skip_all)]
    async fn authenticate(
        &self,
        request: Request<AuthenticateRequest>,
    ) -> Result<Response<AuthenticateResponse>, Status> {
        let AuthenticateRequest { username } = request.into_inner();

        if USER_DB.contains(&username.as_str()) {
            let auth_token = self
                .auth_counter
                .fetch_add(1, Ordering::Relaxed)
                .to_string();

            let auth_token_header = auth_token
                .parse::<HeaderValue>()
                .expect("auth_token is a number so it'll always be valid ASCII");

            self.auth_db
                .write()
                .await
                .insert(auth_token_header, username);

            Ok(Response::new(AuthenticateResponse { auth_token }))
        } else {
            Err(Status::unauthenticated("Unregistered username"))
        }
    }
}

#[derive(Clone)]
struct AuthInterceptor {
    auth_db: AuthDb,
}

#[tonic::async_trait]
impl RequestInterceptor for AuthInterceptor {
    async fn intercept(&self, mut request: HttpRequest<Body>) -> Result<HttpRequest<Body>, Status> {
        let Some(auth_token) = request.headers().get(AUTH_TOKEN_HEADER) else {
            return Err(Status::unauthenticated("Missing authentication token"));
        };

        if let Some(username) = self.auth_db.read().await.get(auth_token) {
            request.extensions_mut().insert(AuthInfo {
                username: username.clone(),
            });

            Ok(request)
        } else {
            Err(Status::unauthenticated("Invalid authentication token"))
        }
    }
}

#[derive(Clone)]
struct AuthInfo {
    username: String,
}

// TODO: Make this a proper database.
type AuthToken = HeaderValue;
type Username = String;
type AuthDb = Arc<RwLock<HashMap<AuthToken, Username>>>;

const USER_DB: &[&str] = &["vini", "julia"];
