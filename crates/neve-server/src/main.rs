use clap::Parser;
use neve_protocol::{
    ChatRequest, ChatResponse, ShareInfoRequest, ShareInfoResponse,
    neve_service_server::{NeveService, NeveServiceServer},
};
use std::{collections::HashMap, net::SocketAddr, pin::Pin};
use tokio::sync::{RwLock, broadcast};
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};
use tonic::{Request, Response, Status, transport::Server};
use tracing::{debug, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .without_time()
        .init();

    let args = ServerArgs::parse();

    let addr = format!("[::]:{}", args.port).parse()?;
    info!(?addr, "Server starting");

    let server = MainNeveServer::default();
    Server::builder()
        .add_service(NeveServiceServer::new(server))
        .serve(addr)
        .await?;

    Ok(())
}

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
            debug!(?addr, "New client");

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

        debug!(?from, ?addr, "Request");

        let messages_tx = self.messages_tx.clone();
        tokio::spawn(async move {
            let mut requests = request.into_inner();

            while let Some(request) = requests.next().await {
                let Ok(request) = request else {
                    continue;
                };

                debug!(msg = ?request.message, ?from, "Broadcasting");

                if messages_tx
                    .send(ChatResponse {
                        from: from.clone(),
                        message: request.message,
                    })
                    .is_err()
                {
                    return;
                }
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

#[derive(Parser)]
struct ServerArgs {
    #[arg(long)]
    port: u16,
}
