use futures::future::try_join_all;
use itertools::Itertools;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::auth::AuthServer;
use neve_server::RowId;

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

    let mut chat_streams = try_join_all(
        accounts
            .into_iter()
            .map(|account_id| chat_server.make_chat_stream(account_id)),
    )
    .await?;

    for i in 0..chat_streams.len() {
        for message in ["hi", "how you guys doing"] {
            chat_streams[i].send(message).await;

            let responses = try_join_all(chat_streams.iter_mut().map(ChatStream::recv)).await?;

            // Every client received the same response
            assert!(responses.iter().all_equal())
        }
    }

    Ok(())
}

impl ChatServer {
    async fn make_chat_stream(&self, account_id: RowId) -> Result<ChatStream, Status> {
        let (messages_tx, messages_rx) = mpsc::channel(2);

        let mut request = Request::new(
            ReceiverStream::new(messages_rx).map(|message| Ok(ChatRequest { message })),
        );
        request.extensions_mut().insert(AuthInfo { account_id });
        let responses = self.chat(request).await?.into_inner();

        Ok(ChatStream {
            requests: messages_tx,
            responses,
        })
    }
}

struct ChatStream {
    requests: mpsc::Sender<String>,
    responses: <ChatServer as ChatService>::ChatStream,
}

impl ChatStream {
    pub async fn send(&self, message: &str) {
        self.requests.send(message.to_owned()).await.unwrap()
    }

    pub async fn recv(&mut self) -> Result<String, Status> {
        self.responses
            .next()
            .await
            .unwrap()
            .map(|response| response.message)
    }
}
