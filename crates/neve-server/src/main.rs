use neve_protocol::{
    ChatRequest, ChatResponse, SERVER_PORT, ShareInfoRequest, ShareInfoResponse,
    neve_service_server::{NeveService, NeveServiceServer},
};
use std::{collections::HashMap, net::SocketAddr, pin::Pin};
use tokio::sync::{RwLock, broadcast};
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};
use tonic::{Request, Response, Status, transport::Server};
use tracing::{debug, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug)]
pub struct MainNeveServer {
    clients: RwLock<HashMap<SocketAddr, String>>,
    messages_tx: broadcast::Sender<ChatResponse>,
    messages_rx: broadcast::Receiver<ChatResponse>,
}

#[tonic::async_trait]
impl NeveService for MainNeveServer {
    #[tracing::instrument(err, skip_all)]
    async fn share_info(
        &self,
        request: Request<ShareInfoRequest>,
    ) -> Result<Response<ShareInfoResponse>, Status> {
        if let Some(addr) = request.remote_addr() {
            debug!(?addr, "Client has shared info");

            self.clients
                .write()
                .await
                .insert(addr, request.into_inner().name);
        }

        Ok(Response::new(ShareInfoResponse {}))
    }

    type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatResponse, Status>> + Send + 'static>>;

    #[tracing::instrument(err, skip_all)]
    async fn chat(
        &self,
        request: Request<tonic::Streaming<ChatRequest>>,
    ) -> Result<Response<Self::ChatStream>, Status> {
        let Some(addr) = request.remote_addr() else {
            return Err(Status::internal("No address :("));
        };

        let Some(from) = self.clients.read().await.get(&addr).cloned() else {
            return Err(Status::unauthenticated("Client hasn't registered yet"));
        };

        info!(?from, ?addr, "Got a chat request");

        let messages_tx = self.messages_tx.clone();
        tokio::spawn(async move {
            let mut requests = request.into_inner();

            while let Some(request) = requests.next().await {
                let Ok(request) = request else {
                    continue;
                };

                _ = messages_tx.send(ChatResponse {
                    from: from.clone(),
                    message: request.message,
                });
            }
        });

        let responses = BroadcastStream::new(self.messages_rx.resubscribe())
            .map(|message| message.map_err(|err| Status::internal(err.to_string())));

        Ok(Response::new(Box::pin(responses)))
    }
}

impl Default for MainNeveServer {
    fn default() -> Self {
        let (messages_tx, messages_rx) = broadcast::channel(128);

        Self {
            clients: Default::default(),
            messages_tx,
            messages_rx,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let addr = format!("[::1]:{SERVER_PORT}").parse()?;
    let server = MainNeveServer::default();

    info!(?addr, "Server will start listening");

    Server::builder()
        .add_service(NeveServiceServer::new(server))
        .serve(addr)
        .await?;

    Ok(())
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().compact())
        .init();
}
