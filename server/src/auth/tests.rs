use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Context;
use futures::future::try_join_all;
use itertools::Itertools;
use tonic::Code;

use super::*;

const USERNAME: &str = "test-username";
const PASSWORD: &str = "test-password";

fn paseto_key_for_tests() -> Arc<PasetoSymmetricKey> {
    Arc::new(PasetoSymmetricKey::from(PasetoKey::from(
        b"test-key-for-neve-auth-tests-123",
    )))
}

#[sqlx::test]
async fn authenticates_after_registering(db: PgPool) -> anyhow::Result<()> {
    let server = AuthServer::for_tests(db);

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
    let server = AuthServer::for_tests(db);

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
    let server = AuthServer::for_tests(db);

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
    let server = AuthServer::for_tests(db);

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
    let server = AuthServer::for_tests(db);

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
async fn interceptor_denies_unauthenticated_requests() -> anyhow::Result<()> {
    let mut interceptor = AuthInterceptor {
        paseto_key: paseto_key_for_tests(),
    };

    let request = Request::new(());

    let error = interceptor.call(request).unwrap_err();

    assert_eq!(error.code(), Code::Unauthenticated);

    Ok(())
}

#[sqlx::test]
async fn interceptor_allows_authenticated_requests() -> anyhow::Result<()> {
    let mut interceptor = AuthInterceptor {
        paseto_key: paseto_key_for_tests(),
    };

    let account_id = 55;
    let auth_token = AuthInfo { account_id }.into_auth_token(&paseto_key_for_tests())?;

    let mut request = Request::new(());
    request
        .metadata_mut()
        .insert(AUTH_TOKEN_HEADER, auth_token.parse()?);

    let request = interceptor.call(request)?;

    let auth_info = request
        .extensions()
        .get::<AuthInfo>()
        .context("A successful auth interception should insert `AuthInfo`")?;

    assert_eq!(auth_info.account_id, account_id);

    Ok(())
}

impl AuthServer {
    pub fn for_tests(db: PgPool) -> Self {
        Self {
            db,
            paseto_key: paseto_key_for_tests(),
        }
    }

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

        let auth_info = AuthInfo::from_auth_token(&auth_token, &self.paseto_key)
            .expect("Auth token must be valid");

        Ok(auth_info.account_id)
    }
}

impl AuthInfo {
    /// Creates a [`Request`] with synthetic [`AuthInfo`] containing the given `account_id`, making it look like it was
    /// requested by that account through the normal [`AuthInterceptor`] flow.
    pub fn request_from<T>(account_id: RowId, value: T) -> Request<T> {
        let mut request = Request::new(value);
        request.extensions_mut().insert(AuthInfo { account_id });
        request
    }
}
