use std::pin::Pin;

use itertools::Itertools;
use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status};
use tracing::{Instrument, instrument};

use crate::{auth::AuthInfo, error::IntoStatus};
use neve_proto::server::v1::{
    CreateChatRequest, CreateChatResponse, GetChatsRequest, GetChatsResponse,
    chat_service_server::{ChatService, ChatServiceServer},
};

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

        let mut tx = self.db.begin().await.map_err(IntoStatus::into_status)?;

        let name = if let Some(name) = name {
            name
        } else {
            let accounts = sqlx::query!(
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
            .map_err(IntoStatus::into_status)?;

            accounts
                .into_iter()
                .map(|account| account.username)
                .join(", ")
        };

        let chat_id = sqlx::query_scalar!(
            r#"
                INSERT INTO chat (name)
                VALUES ($1)
                RETURNING id
            "#,
            name
        )
        .fetch_one(tx.as_mut())
        .await
        .map_err(IntoStatus::into_status)?;

        for account_id in participants {
            sqlx::query!(
                r#"
                    INSERT INTO chat_account (chat_id, account_id)
                    VALUES ($1, $2)
                "#,
                chat_id,
                account_id
            )
            .execute(tx.as_mut())
            .await
            .map_err(IntoStatus::into_status)?;
        }

        tx.commit().await.map_err(IntoStatus::into_status)?;

        Ok(Response::new(CreateChatResponse { chat_id }))
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
                        Ok(chat_account) => Ok(GetChatsResponse {
                            chat_id: chat_account.chat_id,
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
}
