use std::pin::Pin;

use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status};
use tracing::{Instrument, instrument};

use crate::{
    auth::AuthInfo,
    error::IntoStatus,
    message::listener::{ExistingMessages, MessageListener, MessageListenerError},
};
use neve_proto::server::v1::{
    GetFutureMessagesRequest, GetFutureMessagesResponse, GetMessageRequest, GetMessageResponse,
    GetMessagesRequest, GetMessagesResponse, GetPastMessagesRequest, GetPastMessagesResponse,
    SendMessageRequest, SendMessageResponse,
    message_service_server::{MessageService, MessageServiceServer},
};

mod listener;
#[cfg(test)]
mod tests;

pub struct MessageServer {
    db: PgPool,
    message_listener: MessageListener,
}

impl MessageServer {
    pub async fn new(db: PgPool) -> Result<Self, MessageListenerError> {
        let message_listener = MessageListener::listen(db.clone()).await?;
        Ok(Self {
            db,
            message_listener,
        })
    }

    pub fn service(self) -> MessageServiceServer<Self> {
        MessageServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl MessageService for MessageServer {
    #[instrument(skip(self), fields(request = ?request.get_ref()), err)]
    async fn send_message(
        &self,
        request: Request<SendMessageRequest>,
    ) -> Result<Response<SendMessageResponse>, Status> {
        let AuthInfo { account_id } = AuthInfo::from_request(&request)?;

        let SendMessageRequest { chat_id, content } = request.into_inner();

        let mut tx = self.db.begin().await.map_err(IntoStatus::into_status)?;

        // Updating the chat row serializes message position allocation for this chat. The lock is held until the
        // transaction commits, so message position also matches commit order.
        let next_message_position = sqlx::query_scalar!(
            r#"
                UPDATE chat
                SET next_message_position = next_message_position + 1
                WHERE id = $1
                RETURNING next_message_position
            "#,
            chat_id
        )
        .fetch_one(tx.as_mut())
        .await
        .map_err(IntoStatus::into_status)?;

        let message_id = sqlx::query_scalar!(
            r#"
                INSERT INTO message (account_id, chat_id, content, chat_position)
                VALUES ($1, $2, $3, $4)
                RETURNING id
            "#,
            account_id,
            chat_id,
            content,
            next_message_position,
        )
        .fetch_one(tx.as_mut())
        .await
        .map_err(IntoStatus::into_status)?;

        tx.commit().await.map_err(IntoStatus::into_status)?;

        Ok(Response::new(SendMessageResponse { message_id }))
    }

    #[instrument(skip(self), fields(request = ?request.get_ref()), err)]
    async fn get_message(
        &self,
        request: Request<GetMessageRequest>,
    ) -> Result<Response<GetMessageResponse>, Status> {
        let GetMessageRequest { message_id } = request.into_inner();

        let message = sqlx::query!(
            r#"
                SELECT account_id, chat_id, content
                FROM message
                WHERE id = $1
            "#,
            message_id
        )
        .fetch_one(&self.db)
        .await
        .map_err(IntoStatus::into_status)?;

        Ok(Response::new(GetMessageResponse {
            account_id: message.account_id,
            chat_id: message.chat_id,
            content: message.content,
        }))
    }

    type GetPastMessagesStream = Pin<Box<ReceiverStream<Result<GetPastMessagesResponse, Status>>>>;

    #[instrument(skip(self), fields(request = ?request.get_ref()), err)]
    async fn get_past_messages(
        &self,
        request: Request<GetPastMessagesRequest>,
    ) -> Result<Response<Self::GetPastMessagesStream>, Status> {
        let GetPastMessagesRequest { chat_id } = request.into_inner();

        let db = self.db.clone();
        let (responses_tx, responses_rx) = mpsc::channel(1024);
        tokio::spawn(
            async move {
                let mut results = sqlx::query_scalar!(
                    r#"
                        SELECT id
                        FROM message
                        WHERE chat_id = $1
                        ORDER BY chat_position
                    "#,
                    chat_id
                )
                .fetch(&db);

                while let Some(result) = results.next().await {
                    let response = match result {
                        Ok(message_id) => Ok(GetPastMessagesResponse { message_id }),
                        Err(error) => Err(error.into_status()),
                    };

                    if responses_tx.send(response).await.is_err() {
                        return;
                    }
                }
            }
            .in_current_span(),
        );

        Ok(Response::new(Box::pin(ReceiverStream::new(responses_rx))))
    }

    type GetFutureMessagesStream =
        Pin<Box<dyn Stream<Item = Result<GetFutureMessagesResponse, Status>> + Send>>;

    #[instrument(skip(self), fields(request = ?request.get_ref()), err)]
    async fn get_future_messages(
        &self,
        request: Request<GetFutureMessagesRequest>,
    ) -> Result<Response<Self::GetFutureMessagesStream>, Status> {
        let GetFutureMessagesRequest { chat_id } = request.into_inner();

        let responses = self
            .message_listener
            .message_stream(chat_id, ExistingMessages::Exclude)
            .await
            .map_err(IntoStatus::into_status)?
            .map(|result| match result {
                Ok(message_id) => Ok(GetFutureMessagesResponse { message_id }),
                Err(error) => Err(error.into_status()),
            });

        Ok(Response::new(Box::pin(responses)))
    }

    type GetMessagesStream =
        Pin<Box<dyn Stream<Item = Result<GetMessagesResponse, Status>> + Send>>;

    #[instrument(skip(self), fields(request = ?request.get_ref()), err)]
    async fn get_messages(
        &self,
        request: Request<GetMessagesRequest>,
    ) -> Result<Response<Self::GetMessagesStream>, Status> {
        let GetMessagesRequest { chat_id } = request.into_inner();

        let responses = self
            .message_listener
            .message_stream(chat_id, ExistingMessages::Include)
            .await
            .map_err(IntoStatus::into_status)?
            .map(|result| match result {
                Ok(message_id) => Ok(GetMessagesResponse { message_id }),
                Err(error) => Err(error.into_status()),
            });

        Ok(Response::new(Box::pin(responses)))
    }
}
