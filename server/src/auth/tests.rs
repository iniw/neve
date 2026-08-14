use std::sync::atomic::AtomicUsize;

use anyhow::Context;
use futures::future::try_join_all;
use itertools::Itertools;
use tonic::Code;

use super::*;

const USERNAME: &str = "test-username";
const PASSWORD: &str = "test-password";

#[sqlx::test]
async fn authenticates_after_registering(db: PgPool) -> anyhow::Result<()> {
    let server = AuthServer::new(db);

    server
        .register(Request::new(RegisterRequest {
            username: USERNAME.to_owned(),
            password: PASSWORD.to_owned(),
        }))
        .await?;

    server
        .authenticate(Request::new(AuthenticateRequest {
            username: USERNAME.to_owned(),
            password: PASSWORD.to_owned(),
        }))
        .await?;

    Ok(())
}

#[sqlx::test]
async fn doesnt_authenticate_before_registering(db: PgPool) -> anyhow::Result<()> {
    let server = AuthServer::new(db);

    let error = server
        .authenticate(Request::new(AuthenticateRequest {
            username: USERNAME.to_owned(),
            password: PASSWORD.to_owned(),
        }))
        .await
        .unwrap_err();

    assert_eq!(error.code(), Code::NotFound);

    Ok(())
}

#[sqlx::test]
async fn auth_tokens_are_unique(db: PgPool) -> anyhow::Result<()> {
    let server = AuthServer::new(db);

    let usernames = ["julia", "vono", "karks", "gui"];

    let auth_tokens = try_join_all(usernames.into_iter().map(async |username| {
        server
            .register(Request::new(RegisterRequest {
                username: username.to_owned(),
                password: "123".to_owned(),
            }))
            .await?;

        let AuthenticateResponse { auth_token } = server
            .authenticate(Request::new(AuthenticateRequest {
                username: username.to_owned(),
                password: "123".to_owned(),
            }))
            .await?
            .into_inner();

        Ok::<_, Status>(auth_token)
    }))
    .await?;

    assert!(auth_tokens.iter().all_unique());

    Ok(())
}

#[sqlx::test]
async fn password_needs_to_match(db: PgPool) -> anyhow::Result<()> {
    let server = AuthServer::new(db);

    server
        .register(Request::new(RegisterRequest {
            username: USERNAME.to_owned(),
            password: PASSWORD.to_owned(),
        }))
        .await?;

    let error = server
        .authenticate(Request::new(AuthenticateRequest {
            username: USERNAME.to_owned(),
            password: ":p".to_owned(),
        }))
        .await
        .unwrap_err();

    assert_eq!(error.code(), Code::Unauthenticated);

    Ok(())
}

#[sqlx::test]
async fn usernames_are_unique(db: PgPool) -> anyhow::Result<()> {
    let server = AuthServer::new(db);

    server
        .register(Request::new(RegisterRequest {
            username: USERNAME.to_owned(),
            password: PASSWORD.to_owned(),
        }))
        .await?;

    let error = server
        .register(Request::new(RegisterRequest {
            username: USERNAME.to_owned(),
            password: PASSWORD.to_owned(),
        }))
        .await
        .unwrap_err();

    assert_eq!(error.code(), Code::AlreadyExists);

    Ok(())
}

#[sqlx::test]
async fn interceptor_denies_unautheticated_requests() -> anyhow::Result<()> {
    let interceptor = AuthInterceptor {
        auth_db: AuthDb::default(),
    };

    let request = HttpRequest::new(Body::empty());

    let error = interceptor.intercept(request).await.unwrap_err();

    assert_eq!(error.code(), Code::Unauthenticated);

    Ok(())
}

#[sqlx::test]
async fn interceptor_allows_authenticated_requests() -> anyhow::Result<()> {
    let auth_token = HeaderValue::from_static("47");
    let account_id = 55;

    let interceptor = AuthInterceptor {
        auth_db: Arc::new(RwLock::new(HashMap::from([(
            auth_token.clone(),
            account_id,
        )]))),
    };

    let request = HttpRequest::builder()
        .header(AUTH_TOKEN_HEADER, auth_token)
        .body(Body::empty())
        .unwrap();

    let request = interceptor.intercept(request).await?;

    let auth_info = request
        .extensions()
        .get::<AuthInfo>()
        .context("A successful auth interception should insert `AuthInfo`")?;

    assert_eq!(auth_info.account_id, account_id);

    Ok(())
}

impl AuthServer {
    pub async fn generate_account(&self) -> Result<RowId, Status> {
        static GENERATED_ACCOUNT_COUNTER: AtomicUsize = AtomicUsize::new(0);

        let generated_account_id = GENERATED_ACCOUNT_COUNTER.fetch_add(1, Ordering::Relaxed);

        let username = format!("test-{generated_account_id}");
        let password = username.clone();

        self.register(Request::new(RegisterRequest {
            username: username.clone(),
            password: password.clone(),
        }))
        .await?;

        let AuthenticateResponse { auth_token } = self
            .authenticate(Request::new(AuthenticateRequest { username, password }))
            .await?
            .into_inner();

        let auth_token = HeaderValue::from_str(&auth_token)
            .expect("An authentication token is always a valid header value");

        let account_id = self
            .auth_db
            .read()
            .await
            .get(&auth_token)
            .copied()
            .expect("A successful authentication always inserts the account ID");

        Ok(account_id)
    }
}
