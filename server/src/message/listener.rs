use std::{
    collections::HashMap,
    num::ParseIntError,
    sync::{Arc, Weak},
};

use sqlx::{PgPool, postgres::PgListener};
use thiserror::Error;
use tokio::sync::{RwLock, mpsc, watch};
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tokio_util::task::AbortOnDropHandle;
use tracing::Instrument;

use crate::error::IntoStatus;
use neve_server::RowId;

pub struct MessageListener {
    db: PgPool,
    chats: Weak<RwLock<MessageListenerMap>>,
    _listener_task: AbortOnDropHandle<Result<(), MessageListenerError>>,
}

impl MessageListener {
    /// Listens to the `new_message` postgres notification channel to allow reacting to new messages.
    ///
    /// This function just establishes the notification listener, use [`Self::message_stream`] to get a stream of
    /// messages for a specific chat.
    ///
    /// See:
    /// - <https://www.postgresql.org/docs/current/sql-listen.html>
    /// - <https://www.postgresql.org/docs/current/sql-notify.html>
    /// - <https://docs.rs/sqlx/latest/sqlx/postgres/struct.PgListener.html>
    pub async fn listen(db: PgPool) -> Result<Self, MessageListenerError> {
        let mut listener = PgListener::connect_with(&db).await?;
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

                Err(MessageListenerError::ListenerStopped)
            }
            .instrument(tracing::info_span!("listen")),
        ));

        Ok(Self {
            db,
            chats: weak_chats,
            _listener_task: listener_task,
        })
    }

    /// Creates a [`Stream`](tokio_stream::Stream) of messages for a specific chat as they get sent, yielding their ID.
    pub async fn message_stream(
        &self,
        chat_id: RowId,
        existing_messages: ExistingMessages,
    ) -> Result<ReceiverStream<Result<RowId, MessageListenerError>>, MessageListenerError> {
        let mut notifications = self.subscribe(chat_id).await?;

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
                            Err(error) => Err(MessageListenerError::Database(error)),
                        };

                        if responses_tx.send(response).await.is_err() {
                            return;
                        }
                    }

                    if notifications.changed().await.is_err() {
                        _ = responses_tx
                            .send(Err(MessageListenerError::ListenerStopped))
                            .await;

                        return;
                    }
                }
            }
            .in_current_span(),
        );

        Ok(ReceiverStream::new(responses_rx))
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

/// A switch to decide whether to include existing messages in [`MessageListener::message_stream`]'s stream.
pub enum ExistingMessages {
    /// Include existing messages.
    ///
    /// The stream will be a comprehensive list of both past and future messages sent in the chat.
    Include,

    /// Exclude existing messages.
    ///
    /// The stream will only contain future messages sent in the chat.
    Exclude,
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
