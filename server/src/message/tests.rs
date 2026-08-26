use neve_proto::server::v1::{
    CreateChatRequest, CreateChatResponse, chat_service_server::ChatService,
};

use crate::{auth::AuthServer, chat::ChatServer};

use super::*;

#[sqlx::test]
async fn get_message(db: PgPool) -> anyhow::Result<()> {
    let auth_server = AuthServer::for_tests(db.clone());
    let chat_server = ChatServer::new(db.clone());
    let server = MessageServer::new(db);

    let vini = auth_server.generate_account().await?;
    let julia = auth_server.generate_account().await?;

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

    let response = server
        .get_message(Request::new(GetMessageRequest { message_id }))
        .await?
        .into_inner();

    assert_eq!(response.account_id, vini);
    assert_eq!(response.chat_id, chat_id);
    assert_eq!(response.content, content);

    let mut responses = server
        .get_past_messages(Request::new(GetPastMessagesRequest { chat_id }))
        .await?
        .into_inner();

    let response = responses.next().await.expect("A message was just sent")?;

    assert_eq!(response.message_id, message_id);

    Ok(())
}

#[sqlx::test]
async fn get_past_messages(db: PgPool) -> anyhow::Result<()> {
    let auth_server = AuthServer::for_tests(db.clone());
    let chat_server = ChatServer::new(db.clone());
    let server = MessageServer::new(db);

    let vini = auth_server.generate_account().await?;
    let julia = auth_server.generate_account().await?;

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
    let server = MessageServer::new(db);

    let vini = auth_server.generate_account().await?;
    let julia = auth_server.generate_account().await?;

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
        .get_future_messages(Request::new(GetFutureMessagesRequest { chat_id }))
        .await?
        .into_inner();

    for message in ["Hello!", "Bye!"] {
        let content = message.to_owned();

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

        let response = responses.next().await.expect("A message was just sent")?;

        assert_eq!(response.message_id, message_id);
    }

    Ok(())
}

#[sqlx::test]
async fn get_messages(db: PgPool) -> anyhow::Result<()> {
    let auth_server = AuthServer::for_tests(db.clone());
    let chat_server = ChatServer::new(db.clone());
    let server = MessageServer::new(db);

    let vini = auth_server.generate_account().await?;
    let julia = auth_server.generate_account().await?;

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

    let content = "Past".to_owned();
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
        .get_messages(Request::new(GetMessagesRequest { chat_id }))
        .await?
        .into_inner();

    // We get messages from the past
    let response = responses.next().await.expect("A message was just sent")?;
    assert_eq!(response.message_id, message_id);

    // And from the future
    for message in ["Hello!", "Bye!"] {
        let content = message.to_owned();

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

        let response = responses.next().await.expect("A message was just sent")?;
        assert_eq!(response.message_id, message_id);
    }

    Ok(())
}
