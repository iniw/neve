use std::{num::ParseIntError, pin::Pin, str::FromStr};

use itertools::Itertools;
use neve_proto::server::v1::{
    GetFutureMessagesRequest, GetFutureMessagesResponse, GetMessageRequest, GetMessageResponse,
    GetMessagesRequest, GetMessagesResponse, GetPastMessagesRequest, GetPastMessagesResponse,
    SendMessageRequest, SendMessageResponse,
    message_service_server::{MessageService, MessageServiceServer},
};

use sqlx::{PgPool, postgres::PgListener};
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status};
use tracing::{Instrument, instrument, warn};

use neve_server::RowId;

use crate::{auth::AuthInfo, error};

#[cfg(test)]
mod tests;

#[derive(derive_more::Debug)]
pub struct MessageServer {
    #[debug(skip)]
    db: PgPool,
}

impl MessageServer {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub fn service(self) -> MessageServiceServer<Self> {
        MessageServiceServer::new(self)
    }

    /// Constructs a [`Stream`] that produces a value when a new row is inserted in the `message` table of the database.
    ///
    /// The underlying stream of events is the `new_message` postgres notification channel, which is created on the same
    /// migration script that the table itself is created on.
    ///
    /// See:
    /// - <https://www.postgresql.org/docs/current/sql-listen.html>
    /// - <https://www.postgresql.org/docs/current/sql-notify.html>
    /// - <https://docs.rs/sqlx/latest/sqlx/postgres/struct.PgListener.html>
    async fn new_message_stream(
        db: PgPool,
        chat_id: RowId,
    ) -> Result<impl Stream<Item = Result<RowId, sqlx::Error>>, sqlx::Error> {
        let mut listener = PgListener::connect_with(&db).await?;

        listener.listen("new_message").await?;

        Ok(listener.into_stream().filter_map(move |result| {
            match result.and_then(|notification| {
                notification
                    .payload()
                    .parse::<ChatMessage>()
                    .map_err(|error| sqlx::Error::InvalidArgument(error.to_string()))
            }) {
                Ok(chat_message) => {
                    if chat_message.chat_id == chat_id {
                        Some(Ok(chat_message.message_id))
                    } else {
                        None
                    }
                }
                Err(error) => Some(Err(error)),
            }
        }))
    }
}

#[tonic::async_trait]
impl MessageService for MessageServer {
    #[instrument(err)]
    async fn send_message(
        &self,
        request: Request<SendMessageRequest>,
    ) -> Result<Response<SendMessageResponse>, Status> {
        let AuthInfo { account_id } = AuthInfo::from_request(&request)?;

        let SendMessageRequest { chat_id, content } = request.into_inner();

        let record = sqlx::query!(
            r#"
                INSERT INTO message (account_id, chat_id, content)
                VALUES ($1, $2, $3)
                RETURNING id
            "#,
            account_id,
            chat_id,
            content
        )
        .fetch_one(&self.db)
        .await
        .map_err(error::db)?;

        Ok(Response::new(SendMessageResponse {
            message_id: record.id,
        }))
    }

    #[instrument(err)]
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
        .map_err(error::db)?;

        Ok(Response::new(GetMessageResponse {
            account_id: record.account_id,
            chat_id: record.chat_id,
            content: record.content,
        }))
    }

    type GetPastMessagesStream = Pin<Box<ReceiverStream<Result<GetPastMessagesResponse, Status>>>>;

    #[instrument(err)]
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
                    "#,
                    chat_id
                )
                .fetch(&db);

                while let Some(result) = results.next().await {
                    let response = match result {
                        Ok(record) => Ok(GetPastMessagesResponse {
                            message_id: record.id,
                        }),
                        Err(error) => Err(error::db(error)),
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

    #[instrument(err)]
    async fn get_future_messages(
        &self,
        request: Request<GetFutureMessagesRequest>,
    ) -> Result<Response<Self::GetFutureMessagesStream>, Status> {
        let GetFutureMessagesRequest { chat_id } = request.into_inner();

        let responses = Self::new_message_stream(self.db.clone(), chat_id)
            .await
            .map_err(error::db)?
            .map(move |result| match result {
                Ok(message_id) => Ok(GetFutureMessagesResponse { message_id }),
                Err(error) => Err(error::db(error)),
            });

        Ok(Response::new(Box::pin(responses)))
    }

    type GetMessagesStream = Pin<Box<ReceiverStream<Result<GetMessagesResponse, Status>>>>;

    #[instrument(err)]
    async fn get_messages(
        &self,
        request: Request<GetMessagesRequest>,
    ) -> Result<Response<Self::GetMessagesStream>, Status> {
        let GetMessagesRequest { chat_id } = request.into_inner();

        let db = self.db.clone();
        let (responses_tx, responses_rx) = mpsc::channel(1024);

        let mut new_messages = Self::new_message_stream(db.clone(), chat_id)
            .await
            .map_err(error::db)?;

        tokio::spawn(
            async move {
                // First batch: messages from the past (as in already present in the DB at the time of request)
                let mut results = sqlx::query!(
                    r#"
                        SELECT id
                        FROM message
                        WHERE chat_id = $1
                        ORDER BY id
                    "#,
                    chat_id
                )
                .fetch(&db);

                // Track the last message we sent in the first batch to not send it again in the second batch
                //
                // Because row IDs increase monotonically, we can use the last-sent message's ID as the
                // "crossing point" between past and future: any message present in the second batch with an ID
                // smaller than or equal to it has actually already been sent as part of the first batch, and so
                // shouldn't be sent again to avoid producing duplicate responses.
                //
                // This can happen when a message is sent after subscribing to `self.messages_tx` but before executing
                // the sqlx query.
                //
                // NOTE: This is actually not really true at all and we can't guarantee that row ID ordering corresponds
                // insertion ordering because insertions operate on transactions and those are batched and executed by
                // postgres' will.
                //
                // It works for now though! We can come up with something smarter later.
                let mut first_batch_last_message_id = None;

                while let Some(result) = results.next().await {
                    let response = match result {
                        Ok(record) => {
                            first_batch_last_message_id = Some(record.id);

                            Ok(GetMessagesResponse {
                                message_id: record.id,
                            })
                        }
                        Err(error) => Err(error::db(error)),
                    };

                    if responses_tx.send(response).await.is_err() {
                        return;
                    }
                }

                // Second batch: messages from the future (as in not present in the DB at the time of the request)
                while let Some(result) = new_messages.next().await {
                    let response = match result {
                        Ok(message_id) => {
                            if first_batch_last_message_id
                                .is_none_or(|last_message_id| message_id > last_message_id)
                            {
                                Ok(GetMessagesResponse { message_id })
                            } else {
                                continue;
                            }
                        }
                        Err(error) => {
                            warn!(?error, "Notification failure");
                            Err(Status::internal("Failed to receive message"))
                        }
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
}

#[derive(Clone, Copy)]
struct ChatMessage {
    chat_id: RowId,
    message_id: RowId,
}

impl FromStr for ChatMessage {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let ids: Vec<i32> = s.split(',').map(str::parse).try_collect()?;

        Ok(Self {
            chat_id: ids[0],
            message_id: ids[1],
        })
    }
}
