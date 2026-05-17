use neve_protocol::{
    SERVER_PORT, ShareInfoRequest, ShareInfoResponse,
    neve_service_server::{NeveService, NeveServiceServer},
};
use std::{collections::HashMap, net::SocketAddr};
use tokio::sync::RwLock;
use tonic::{Request, Response, Status, transport::Server};
use tracing::{debug, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Default)]
pub struct MainNeveServer {
    clients: RwLock<HashMap<SocketAddr, String>>,
}

#[tonic::async_trait]
impl NeveService for MainNeveServer {
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
