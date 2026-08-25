use std::pin::Pin;

use itertools::Itertools;
use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status};
use tracing::{Instrument, instrument};

use neve_proto::server::v1::{
    CreateChatRequest, CreateChatResponse, GetChatsRequest, GetChatsResponse,
    chat_service_server::{ChatService, ChatServiceServer},
};

use crate::{auth::AuthInfo, error};

#[cfg(test)]
mod tests;

pub struct ChatServer {
    db: PgPool,
}

impl ChatServer {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub fn service(self) -> ChatServiceServer<Self> {
        ChatServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl ChatService for ChatServer {
    #[instrument(skip(self), fields(request = ?request.get_ref()), err)]
    async fn create_chat(
        &self,
        request: Request<CreateChatRequest>,
    ) -> Result<Response<CreateChatResponse>, Status> {
        let AuthInfo { account_id } = AuthInfo::from_request(&request)?;

        let CreateChatRequest {
            mut participants,
            name,
        } = request.into_inner();

        // The creator of the chat is itself a participant
        participants.push(account_id);

        let mut tx = self.db.begin().await.map_err(error::db)?;

        let name = if let Some(name) = name {
            name
        } else {
            let records = sqlx::query!(
                r#"
                    SELECT username
                    FROM account
                    WHERE id = ANY ($1)
                    LIMIT 5
                "#,
                &participants,
            )
            .fetch_all(tx.as_mut())
            .await
            .map_err(error::db)?;

            records.into_iter().map(|record| record.username).join(", ")
        };

        let chat = sqlx::query!(
            r#"
                INSERT INTO chat (name)
                VALUES ($1)
                RETURNING id
            "#,
            name
        )
        .fetch_one(tx.as_mut())
        .await
        .map_err(error::db)?;

        for account_id in participants {
            sqlx::query!(
                r#"
                    INSERT INTO chat_account (chat_id, account_id)
                    VALUES ($1, $2)
                "#,
                chat.id,
                account_id
            )
            .execute(tx.as_mut())
            .await
            .map_err(error::db)?;
        }

        tx.commit().await.map_err(error::db)?;

        Ok(Response::new(CreateChatResponse { chat_id: chat.id }))
    }

    type GetChatsStream = Pin<Box<ReceiverStream<Result<GetChatsResponse, Status>>>>;

    #[instrument(skip(self), fields(request = ?request.get_ref()), err)]
    async fn get_chats(
        &self,
        request: Request<GetChatsRequest>,
    ) -> Result<Response<Self::GetChatsStream>, Status> {
        let AuthInfo { account_id } = AuthInfo::from_request(&request)?;

        let db = self.db.clone();
        let (responses_tx, responses_rx) = mpsc::channel(10);
        tokio::spawn(
            async move {
                let mut results = sqlx::query!(
                    r#"
                        SELECT chat_id
                        FROM chat_account
                        WHERE account_id = $1
                    "#,
                    account_id
                )
                .fetch(&db);

                while let Some(result) = results.next().await {
                    let response = match result {
                        Ok(record) => Ok(GetChatsResponse {
                            chat_id: record.chat_id,
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
}
