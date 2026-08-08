use std::collections::HashSet;

use futures::future::try_join_all;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::auth::AuthServer;

use super::*;

#[sqlx::test]
async fn all_clients_receive_message(db: PgPool) -> anyhow::Result<()> {
    let auth_server = AuthServer::new(db.clone());
    let chat_server = ChatServer::new(db);

    let accounts = [
        auth_server.generate_account().await?,
        auth_server.generate_account().await?,
        auth_server.generate_account().await?,
    ];

    let mut chat_handles = try_join_all(
        accounts
            .into_iter()
            .map(|account_id| chat_server.start_chatting(account_id)),
    )
    .await?;

    for chatter in 0..chat_handles.len() {
        for message in ["hi", "how you guys doing"] {
            chat_handles[chatter].send_message(message).await;

            let responses =
                try_join_all(chat_handles.iter_mut().map(ChatHandle::receive_message)).await?;

            // Every client received the same response
            let unique_responses = responses.into_iter().collect::<HashSet<_>>();
            assert_eq!(unique_responses.len(), 1);
        }
    }

    Ok(())
}

impl ChatServer {
    async fn start_chatting(&self, account_id: i64) -> Result<ChatHandle, Status> {
        let (messages_tx, messages_rx) = mpsc::channel(2);

        let mut request = Request::new(
            ReceiverStream::new(messages_rx).map(|message| Ok(ChatRequest { message })),
        );
        request.extensions_mut().insert(AuthInfo { account_id });
        let responses = self.chat(request).await?.into_inner();

        Ok(ChatHandle {
            messages_tx,
            responses,
        })
    }
}

struct ChatHandle {
    messages_tx: mpsc::Sender<String>,
    responses: <ChatServer as ChatService>::ChatStream,
}

impl ChatHandle {
    pub async fn send_message(&self, message: &str) {
        self.messages_tx.send(message.to_owned()).await.unwrap()
    }

    pub async fn receive_message(&mut self) -> Result<String, Status> {
        self.responses
            .next()
            .await
            .unwrap()
            .map(|response| response.message)
    }
}
