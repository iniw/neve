use crate::{auth::AuthServer, chat::ChatServer};
use futures::{TryStreamExt, stream::FuturesUnordered};
use neve_proto::server::v1::{
    CreateChatRequest, CreateChatResponse, chat_service_server::ChatService,
};

use super::*;

#[sqlx::test]
async fn get_message(db: PgPool) -> anyhow::Result<()> {
    let auth_server = AuthServer::for_tests(db.clone());
    let chat_server = ChatServer::new(db.clone());
    let server = MessageServer::new(db).await?;

    let AuthInfo { account_id: vini } = auth_server.generate_authenticated_account().await?;
    let AuthInfo { account_id: julia } = auth_server.generate_authenticated_account().await?;

    let CreateChatResponse { chat_id } = chat_server
        .create_chat(AuthInfo::request_from(
            vini,
            CreateChatRequest {
                participants: vec![julia],
                name: None,
            },
        ))
        .await?
        .into_inner();

    let content = "Hello!".to_owned();

    let SendMessageResponse { message_id } = server
        .send_message(AuthInfo::request_from(
            vini,
            SendMessageRequest {
                chat_id,
                content: content.clone(),
            },
        ))
        .await?
        .into_inner();

    let response = server
        .get_message(Request::new(GetMessageRequest { message_id }))
        .await?
        .into_inner();

    assert_eq!(response.account_id, vini);
    assert_eq!(response.chat_id, chat_id);
    assert_eq!(response.content, content);

    Ok(())
}

#[sqlx::test]
async fn get_past_messages(db: PgPool) -> anyhow::Result<()> {
    let auth_server = AuthServer::for_tests(db.clone());
    let chat_server = ChatServer::new(db.clone());
    let server = MessageServer::new(db).await?;

    let AuthInfo { account_id: vini } = auth_server.generate_authenticated_account().await?;
    let AuthInfo { account_id: julia } = auth_server.generate_authenticated_account().await?;

    let CreateChatResponse { chat_id } = chat_server
        .create_chat(AuthInfo::request_from(
            vini,
            CreateChatRequest {
                participants: vec![julia],
                name: None,
            },
        ))
        .await?
        .into_inner();

    let mut responses = server
        .get_past_messages(Request::new(GetPastMessagesRequest { chat_id }))
        .await?
        .into_inner();

    assert!(
        responses.next().await.is_none(),
        "No messages have been sent yet"
    );

    let content = "Hello!".to_owned();

    let SendMessageResponse { message_id } = server
        .send_message(AuthInfo::request_from(
            vini,
            SendMessageRequest {
                chat_id,
                content: content.clone(),
            },
        ))
        .await?
        .into_inner();

    let mut responses = server
        .get_past_messages(Request::new(GetPastMessagesRequest { chat_id }))
        .await?
        .into_inner();

    let response = responses.next().await.expect("A message was just sent")?;

    assert_eq!(response.message_id, message_id);

    assert!(
        responses.next().await.is_none(),
        "No more messages have been sent"
    );

    Ok(())
}

#[sqlx::test]
async fn get_future_messages(db: PgPool) -> anyhow::Result<()> {
    let auth_server = AuthServer::for_tests(db.clone());
    let chat_server = ChatServer::new(db.clone());
    let server = MessageServer::new(db).await?;

    let AuthInfo { account_id: vini } = auth_server.generate_authenticated_account().await?;
    let AuthInfo { account_id: julia } = auth_server.generate_authenticated_account().await?;

    let CreateChatResponse { chat_id } = chat_server
        .create_chat(AuthInfo::request_from(
            vini,
            CreateChatRequest {
                participants: vec![julia],
                name: None,
            },
        ))
        .await?
        .into_inner();

    server
        .send_message(AuthInfo::request_from(
            vini,
            SendMessageRequest {
                chat_id,
                content: "Past".to_owned(),
            },
        ))
        .await?;

    let mut responses = server
        .get_future_messages(Request::new(GetFutureMessagesRequest { chat_id }))
        .await?
        .into_inner();

    let sent_messages = ["Hello!", "Yes!", "Bye!"]
        .into_iter()
        .map(|message| {
            server.send_message(AuthInfo::request_from(
                vini,
                SendMessageRequest {
                    chat_id,
                    content: message.to_owned(),
                },
            ))
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect::<Vec<_>>()
        .await?;

    for sent_message in sent_messages {
        let response = responses.next().await.expect("A message was just sent")?;
        assert_eq!(response.message_id, sent_message.get_ref().message_id);
    }

    Ok(())
}

#[sqlx::test]
async fn get_messages(db: PgPool) -> anyhow::Result<()> {
    let auth_server = AuthServer::for_tests(db.clone());
    let chat_server = ChatServer::new(db.clone());
    let server = MessageServer::new(db).await?;

    let AuthInfo { account_id: vini } = auth_server.generate_authenticated_account().await?;
    let AuthInfo { account_id: julia } = auth_server.generate_authenticated_account().await?;

    let CreateChatResponse { chat_id } = chat_server
        .create_chat(AuthInfo::request_from(
            vini,
            CreateChatRequest {
                participants: vec![julia],
                name: None,
            },
        ))
        .await?
        .into_inner();

    let past_messages = (0..16)
        .map(|index| {
            server.send_message(AuthInfo::request_from(
                vini,
                SendMessageRequest {
                    chat_id,
                    content: format!("Past {index}"),
                },
            ))
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect::<Vec<_>>()
        .await?;

    let mut responses = server
        .get_messages(Request::new(GetMessagesRequest { chat_id }))
        .await?
        .into_inner();

    // We get messages from the past
    for message in past_messages {
        let response = responses.next().await.expect("A message was just sent")?;
        assert_eq!(response.message_id, message.get_ref().message_id);
    }

    let future_messages = (0..16)
        .map(|index| {
            server.send_message(AuthInfo::request_from(
                vini,
                SendMessageRequest {
                    chat_id,
                    content: format!("Message {index}"),
                },
            ))
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect::<Vec<_>>()
        .await?;

    // And from the future
    for message in future_messages {
        let response = responses.next().await.expect("A message was just sent")?;
        assert_eq!(response.message_id, message.get_ref().message_id);
    }

    Ok(())
}
