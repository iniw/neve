use crate::{auth::AuthServer, chat::ChatServer};
use neve_proto::server::v1::{
    CreateChatRequest, CreateChatResponse, chat_service_server::ChatService,
};

use super::*;

#[sqlx::test]
async fn get_message(db: PgPool) -> anyhow::Result<()> {
    let auth_server = AuthServer::for_tests(db.clone());
    let chat_server = ChatServer::new(db.clone());
    let server = MessageServer::new(db).await?;

    let vini = auth_server.test_account().await?;
    let julia = auth_server.test_account().await?;

    let CreateChatResponse { chat_id } = chat_server
        .create_chat(vini.request(CreateChatRequest {
            participants: vec![julia.account_id],
            name: None,
        }))
        .await?
        .into_inner();

    let content = "Hello!".to_owned();

    let SendMessageResponse { message_id } = server
        .send_message(vini.request(SendMessageRequest {
            chat_id,
            content: content.clone(),
        }))
        .await?
        .into_inner();

    let response = server
        .get_message(Request::new(GetMessageRequest { message_id }))
        .await?
        .into_inner();

    assert_eq!(response.account_id, vini.account_id);
    assert_eq!(response.chat_id, chat_id);
    assert_eq!(response.content, content);

    Ok(())
}

#[sqlx::test]
async fn get_past_messages(db: PgPool) -> anyhow::Result<()> {
    let auth_server = AuthServer::for_tests(db.clone());
    let chat_server = ChatServer::new(db.clone());
    let server = MessageServer::new(db).await?;

    let vini = auth_server.test_account().await?;
    let julia = auth_server.test_account().await?;

    let CreateChatResponse { chat_id } = chat_server
        .create_chat(vini.request(CreateChatRequest {
            participants: vec![julia.account_id],
            name: None,
        }))
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
        .send_message(vini.request(SendMessageRequest {
            chat_id,
            content: content.clone(),
        }))
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

    let vini = auth_server.test_account().await?;
    let julia = auth_server.test_account().await?;

    let CreateChatResponse { chat_id } = chat_server
        .create_chat(vini.request(CreateChatRequest {
            participants: vec![julia.account_id],
            name: None,
        }))
        .await?
        .into_inner();

    server
        .send_message(vini.request(SendMessageRequest {
            chat_id,
            content: "Past".to_owned(),
        }))
        .await?;

    let mut responses = server
        .get_future_messages(Request::new(GetFutureMessagesRequest { chat_id }))
        .await?
        .into_inner();

    let mut sent_messages = Vec::new();
    for message in ["Hello!", "Yes!", "Bye!"] {
        let response = server
            .send_message(vini.request(SendMessageRequest {
                chat_id,
                content: message.to_owned(),
            }))
            .await?;
        sent_messages.push(response);
    }

    for message in sent_messages {
        let response = responses.next().await.expect("A message was just sent")?;
        assert_eq!(response.message_id, message.get_ref().message_id);
    }

    Ok(())
}

#[sqlx::test]
async fn get_messages(db: PgPool) -> anyhow::Result<()> {
    let auth_server = AuthServer::for_tests(db.clone());
    let chat_server = ChatServer::new(db.clone());
    let server = MessageServer::new(db).await?;

    let vini = auth_server.test_account().await?;
    let julia = auth_server.test_account().await?;

    let CreateChatResponse { chat_id } = chat_server
        .create_chat(vini.request(CreateChatRequest {
            participants: vec![julia.account_id],
            name: None,
        }))
        .await?
        .into_inner();

    let mut past_messages = Vec::new();
    for n in 0..16 {
        let response = server
            .send_message(vini.request(SendMessageRequest {
                chat_id,
                content: format!("Past {n}"),
            }))
            .await?;
        past_messages.push(response);
    }

    let mut responses = server
        .get_messages(Request::new(GetMessagesRequest { chat_id }))
        .await?
        .into_inner();

    // We get messages from the past
    for message in past_messages {
        let response = responses.next().await.expect("A message was just sent")?;
        assert_eq!(response.message_id, message.get_ref().message_id);
    }

    let mut future_messages = Vec::new();
    for n in 0..16 {
        let response = server
            .send_message(vini.request(SendMessageRequest {
                chat_id,
                content: format!("Message {n}"),
            }))
            .await?;
        future_messages.push(response);
    }

    // And from the future
    for message in future_messages {
        let response = responses.next().await.expect("A message was just sent")?;
        assert_eq!(response.message_id, message.get_ref().message_id);
    }

    Ok(())
}
