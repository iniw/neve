use crate::auth::AuthServer;

use super::*;

#[sqlx::test]
async fn create_then_get(db: PgPool) -> anyhow::Result<()> {
    let auth_server = AuthServer::for_tests(db.clone());
    let server = ChatServer::new(db);

    let AuthInfo { account_id: vini } = auth_server.generate_authenticated_account().await?;
    let AuthInfo { account_id: julia } = auth_server.generate_authenticated_account().await?;

    let CreateChatResponse { chat_id } = server
        .create_chat(AuthInfo::request_from(
            vini,
            CreateChatRequest {
                participants: vec![julia],
                name: None,
            },
        ))
        .await?
        .into_inner();

    assert_eq!(chat_id, 1);

    let mut responses = server
        .get_chats(AuthInfo::request_from(vini, GetChatsRequest {}))
        .await?
        .into_inner();

    let response = responses.next().await.expect("A chat was just created")?;

    assert_eq!(response.chat_id, 1);

    assert!(
        responses.next().await.is_none(),
        "More than one chat was returned even though only one was created"
    );

    Ok(())
}
