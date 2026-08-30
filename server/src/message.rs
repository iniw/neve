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

use neve_proto::server::v1::{
    GetFutureMessagesRequest, GetFutureMessagesResponse, GetMessageRequest, GetMessageResponse,
    GetMessagesRequest, GetMessagesResponse, GetPastMessagesRequest, GetPastMessagesResponse,
    SendMessageRequest, SendMessageResponse,
    message_service_server::{MessageService, MessageServiceServer},
};
use neve_server::RowId;

use crate::{auth::AuthInfo, error::IntoStatus};

#[cfg(test)]
mod tests;

pub struct MessageServer {
    db: PgPool,
    message_notifications: MessageNotifications,
}

impl MessageServer {
    pub async fn new(db: PgPool) -> Result<Self, MessageNotificationError> {
        let message_notifications = MessageNotifications::listen(&db).await?;
        Ok(Self {
            db,
            message_notifications,
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
        let chat = sqlx::query!(
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

        let record = sqlx::query!(
            r#"
                INSERT INTO message (account_id, chat_id, content, chat_position)
                VALUES ($1, $2, $3, $4)
                RETURNING id
            "#,
            account_id,
            chat_id,
            content,
            chat.next_message_position,
        )
        .fetch_one(tx.as_mut())
        .await
        .map_err(IntoStatus::into_status)?;

        tx.commit().await.map_err(IntoStatus::into_status)?;

        Ok(Response::new(SendMessageResponse {
            message_id: record.id,
        }))
    }

    #[instrument(skip(self), fields(request = ?request.get_ref()), err)]
    async fn get_message(
        &self,
        request: Request<GetMessageRequest>,
    ) -> Result<Response<GetMessageResponse>, Status> {
        let GetMessageRequest { message_id } = request.into_inner();

        let record = sqlx::query!(
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
            account_id: record.account_id,
            chat_id: record.chat_id,
            content: record.content,
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
                let mut results = sqlx::query!(
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
                        Ok(record) => Ok(GetPastMessagesResponse {
                            message_id: record.id,
                        }),
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
    ) -> Result<ReceiverStream<Result<RowId, Status>>, MessageNotificationError> {
        let mut notifications = self.message_notifications.subscribe(chat_id).await?;

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
                    let mut records = sqlx::query!(
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

                    while let Some(result) = records.next().await {
                        let response = match result {
                            Ok(record) => {
                                current_chat_position = record.chat_position;
                                Ok(record.id)
                            }
                            Err(error) => Err(error.into_status()),
                        };

                        if responses_tx.send(response).await.is_err() {
                            return;
                        }
                    }

                    if notifications.changed().await.is_err() {
                        let error = MessageNotificationError::ListenerStopped;
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

struct MessageNotifications {
    chats: Weak<RwLock<MessageNotificationMap>>,
    _listener_task: AbortOnDropHandle<Result<(), MessageNotificationError>>,
}

impl MessageNotifications {
    async fn listen(db: &PgPool) -> Result<Self, MessageNotificationError> {
        let mut listener = PgListener::connect_with(db).await?;
        listener.listen("new_message").await?;

        let chats = Arc::new(RwLock::new(MessageNotificationMap::new()));
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

    async fn subscribe(
        &self,
        chat_id: RowId,
    ) -> Result<watch::Receiver<()>, MessageNotificationError> {
        let chats = self
            .chats
            .upgrade()
            .ok_or(MessageNotificationError::ListenerStopped)?;

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
type MessageNotificationMap = HashMap<RowId, watch::Sender<()>>;

#[derive(Debug, Error)]
pub enum MessageNotificationError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Invalid chat ID in message notification: {0}")]
    InvalidChatId(#[from] ParseIntError),

    #[error("Message notification listener stopped")]
    ListenerStopped,
}

impl IntoStatus for MessageNotificationError {
    const MESSAGE: &'static str = "Message notification error";
}
