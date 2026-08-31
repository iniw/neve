use std::{
    collections::HashMap,
    num::ParseIntError,
    pin::Pin,
    sync::{Arc, Weak},
};

use sqlx::{PgPool, postgres::PgListener};
use thiserror::Error;
use tokio::sync::{RwLock, mpsc, watch};
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tokio_util::task::AbortOnDropHandle;
use tonic::{Request, Response, Status};
use tracing::{Instrument, instrument};

use crate::{auth::AuthInfo, error::IntoStatus};
use neve_proto::server::v1::{
    GetFutureMessagesRequest, GetFutureMessagesResponse, GetMessageRequest, GetMessageResponse,
    GetMessagesRequest, GetMessagesResponse, GetPastMessagesRequest, GetPastMessagesResponse,
    SendMessageRequest, SendMessageResponse,
    message_service_server::{MessageService, MessageServiceServer},
};
use neve_server::RowId;

#[cfg(test)]
mod tests;

pub struct MessageServer {
    db: PgPool,
    message_listener: MessageListener,
}

impl MessageServer {
    pub async fn new(db: PgPool) -> Result<Self, MessageListenerError> {
        let message_listener = MessageListener::listen(&db).await?;
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
            .message_id_stream(chat_id, ExistingMessages::Exclude)
            .await
            .map_err(IntoStatus::into_status)?
            .map(|result| result.map(|message_id| GetFutureMessagesResponse { message_id }));

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
            .message_id_stream(chat_id, ExistingMessages::Include)
            .await
            .map_err(IntoStatus::into_status)?
            .map(|result| result.map(|message_id| GetMessagesResponse { message_id }));

        Ok(Response::new(Box::pin(responses)))
    }
}

impl MessageServer {
    async fn message_id_stream(
        &self,
        chat_id: RowId,
        existing_messages: ExistingMessages,
    ) -> Result<ReceiverStream<Result<RowId, Status>>, MessageListenerError> {
        let mut notifications = self.message_listener.subscribe(chat_id).await?;

        let mut current_chat_position = match existing_messages {
            ExistingMessages::Include => 0,
            ExistingMessages::Exclude => sqlx::query_scalar!(
                r#"
                    SELECT next_message_position
                    FROM chat
                    WHERE id = $1
                "#,
                chat_id,
            )
            .fetch_optional(&self.db)
            .await?
            .unwrap_or(0),
        };

        let db = self.db.clone();
        let (responses_tx, responses_rx) = mpsc::channel(1024);
        tokio::spawn(
            async move {
                loop {
                    let mut results = sqlx::query!(
                        r#"
                            SELECT id, chat_position
                            FROM message
                            WHERE chat_id = $1 AND chat_position > $2
                            ORDER BY chat_position
                        "#,
                        chat_id,
                        current_chat_position,
                    )
                    .fetch(&db);

                    while let Some(result) = results.next().await {
                        let response = match result {
                            Ok(message) => {
                                current_chat_position = message.chat_position;
                                Ok(message.id)
                            }
                            Err(error) => Err(error.into_status()),
                        };

                        if responses_tx.send(response).await.is_err() {
                            return;
                        }
                    }

                    if notifications.changed().await.is_err() {
                        let error = MessageListenerError::ListenerStopped;
                        if responses_tx.send(Err(error.into_status())).await.is_err() {
                            return;
                        }
                    }
                }
            }
            .in_current_span(),
        );

        Ok(ReceiverStream::new(responses_rx))
    }
}

enum ExistingMessages {
    Include,
    Exclude,
}

struct MessageListener {
    chats: Weak<RwLock<MessageListenerMap>>,
    _listener_task: AbortOnDropHandle<Result<(), MessageListenerError>>,
}

impl MessageListener {
    async fn listen(db: &PgPool) -> Result<Self, MessageListenerError> {
        let mut listener = PgListener::connect_with(db).await?;
        listener.listen("new_message").await?;

        let chats = Arc::new(RwLock::new(MessageListenerMap::new()));
        let weak_chats = Arc::downgrade(&chats);

        let listener_task = AbortOnDropHandle::new(tokio::spawn(
            async move {
                let mut notifications = listener.into_stream();

                while let Some(notification) = notifications.next().await {
                    let chat_id = notification?.payload().parse::<RowId>()?;

                    if let Some(notifications_tx) = chats.read().await.get(&chat_id) {
                        let _ = notifications_tx.send(());
                    }
                }

                Ok(())
            }
            .instrument(tracing::info_span!("listen")),
        ));

        Ok(Self {
            chats: weak_chats,
            _listener_task: listener_task,
        })
    }

    async fn subscribe(&self, chat_id: RowId) -> Result<watch::Receiver<()>, MessageListenerError> {
        let chats = self
            .chats
            .upgrade()
            .ok_or(MessageListenerError::ListenerStopped)?;

        // Optimistic read first: if this chat has already been subscribed to before we can avoid taking a writer lock.
        if let Some(notifications_tx) = chats.read().await.get(&chat_id) {
            return Ok(notifications_tx.subscribe());
        }

        Ok(chats
            .write()
            .await
            .entry(chat_id)
            .or_insert_with(|| watch::channel(()).0)
            .subscribe())
    }
}

/// Maps a chat ID to a notification sender for new messages in that chat.
type MessageListenerMap = HashMap<RowId, watch::Sender<()>>;

#[derive(Debug, Error)]
pub enum MessageListenerError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Invalid chat ID in message: {0}")]
    InvalidChatId(#[from] ParseIntError),

    #[error("Message listener stopped")]
    ListenerStopped,
}

impl IntoStatus for MessageListenerError {
    const MESSAGE: &'static str = "Message listener error";
}
