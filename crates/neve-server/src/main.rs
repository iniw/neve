use clap::Parser;
use neve_protocol::{
    AUTH_TOKEN_KEY, AuthenticateRequest, AuthenticateResponse, ChatRequest, ChatResponse,
    auth_service_server::{AuthService, AuthServiceServer},
    chat_service_server::{ChatService, ChatServiceServer},
};
use std::{
    collections::HashMap,
    pin::Pin,
    sync::{
        Arc, RwLock,
        atomic::{AtomicU32, Ordering},
    },
};
use tokio::sync::broadcast;
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};
use tonic::{Request, Response, Status, metadata::AsciiMetadataValue, transport::Server};
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
    let chat_server = ChatServer::new(auth_db.clone());

    let check_auth = move |request: Request<()>| {
        let Some(auth_token) = request.metadata().get(AUTH_TOKEN_KEY) else {
            return Err(Status::unauthenticated("Missing authentication token"));
        };

        let Ok(auth_db) = auth_db.read() else {
            return Err(Status::internal("Internal lock poisoning"));
        };

        if auth_db.contains_key(auth_token) {
            Ok(request)
        } else {
            Err(Status::unauthenticated("Invalid authentication token"))
        }
    };

    Server::builder()
        .add_service(AuthServiceServer::new(auth_server))
        .add_service(ChatServiceServer::with_interceptor(chat_server, check_auth))
        .serve(addr)
        .await?;

    Ok(())
}

// TODO: Make this a proper database.
type AuthToken = AsciiMetadataValue;
type Username = String;
type AuthDb = Arc<RwLock<HashMap<AuthToken, Username>>>;

struct ChatServer {
    auth_db: AuthDb,

    messages_tx: broadcast::Sender<ChatResponse>,
    messages_rx: broadcast::Receiver<ChatResponse>,
}

impl ChatServer {
    fn new(auth_db: AuthDb) -> Self {
        let (messages_tx, messages_rx) = broadcast::channel(128);

        Self {
            auth_db,
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
        let Some(auth_token) = request.metadata().get(AUTH_TOKEN_KEY) else {
            return Err(Status::unauthenticated("Missing authentication token"));
        };

        let Ok(auth_db) = self.auth_db.read() else {
            return Err(Status::internal("Internal lock poisoning"));
        };

        let Some(from) = auth_db.get(auth_token).cloned() else {
            return Err(Status::unauthenticated("Not authenticated"));
        };

        debug!(connected = ?from);

        let messages_tx = self.messages_tx.clone();
        tokio::spawn(
            async move {
                let mut request_stream = request.into_inner();

                while let Some(request) = request_stream.next().await {
                    let Ok(ChatRequest { message }) = request else {
                        continue;
                    };

                    debug!(?message, ?from);

                    if messages_tx
                        .send(ChatResponse {
                            from: from.clone(),
                            message,
                        })
                        .is_err()
                    {
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

const USER_DB: &[&str] = &["vini", "julia"];

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
            let Ok(mut auth_db) = self.auth_db.write() else {
                return Err(Status::internal("Internal lock poisoning"));
            };

            let auth_token = self
                .auth_counter
                .fetch_add(1, Ordering::Relaxed)
                .to_string();

            let auth_token_metadata_value = auth_token
                .parse::<AsciiMetadataValue>()
                .expect("auth_token is a number so it'll always be valid ASCII");

            auth_db.insert(auth_token_metadata_value, username);

            Ok(Response::new(AuthenticateResponse { auth_token }))
        } else {
            Err(Status::unauthenticated("Unregistered username"))
        }
    }
}
