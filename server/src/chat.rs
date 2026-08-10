use std::pin::Pin;

use sqlx::PgPool;
use tokio::sync::broadcast;
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};
use tonic::{Request, Response, Status};
use tracing::{Instrument, debug};

use neve_proto::server::v1::{
    ChatRequest, ChatResponse,
    chat_service_server::{ChatService, ChatServiceServer},
};

use crate::{auth::AuthInfo, error};

#[cfg(test)]
mod tests;

pub struct ChatServer {
    db: PgPool,

    messages_tx: broadcast::Sender<ChatResponse>,
    messages_rx: broadcast::Receiver<ChatResponse>,
}

impl ChatServer {
    pub fn new(db: PgPool) -> Self {
        let (messages_tx, messages_rx) = broadcast::channel(128);

        Self {
            db,
            messages_tx,
            messages_rx,
        }
    }

    pub fn service(self) -> ChatServiceServer<Self> {
        ChatServiceServer::new(self)
    }

    async fn chat<S>(
        &self,
        request: Request<S>,
    ) -> Result<Response<<Self as ChatService>::ChatStream>, Status>
    where
        S: Stream<Item = Result<ChatRequest, Status>> + Send + Unpin + 'static,
    {
        let auth_info = request
            .extensions()
            .get::<AuthInfo>()
            .ok_or(Status::unauthenticated("No authentication info"))?;

        debug!(?auth_info.account_id, "New connection");

        let account = sqlx::query!(
            r#"
                select username
                from account
                where id = $1
                limit 1
            "#,
            auth_info.account_id
        )
        .fetch_optional(&self.db)
        .await
        .map_err(error::db())?
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

#[tonic::async_trait]
impl ChatService for ChatServer {
    type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatResponse, Status>> + Send + 'static>>;

    #[tracing::instrument(skip_all, err)]
    async fn chat(
        &self,
        request: Request<tonic::Streaming<ChatRequest>>,
    ) -> Result<Response<Self::ChatStream>, Status> {
        self.chat(request).await
    }
}
