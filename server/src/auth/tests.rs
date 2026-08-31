use std::sync::atomic::{AtomicUsize, Ordering};

use futures::future::try_join_all;
use itertools::Itertools;
use tonic::Code;

use super::*;

const USERNAME: &str = "test-username";
const PASSWORD: &str = "test-password";

fn paseto_key_for_tests() -> Arc<PasetoSymmetricKey> {
    Arc::new(PasetoSymmetricKey::from(PasetoKey::from(
        b"synthetic-key-for-neve-tests-123",
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

#[test]
fn interceptor_denies_unauthenticated_requests() -> anyhow::Result<()> {
    let mut interceptor = AuthInterceptor {
        paseto_key: paseto_key_for_tests(),
    };

    let request = Request::new(());

    let error = interceptor.call(request).unwrap_err();

    assert_eq!(error.code(), Code::Unauthenticated);

    Ok(())
}

#[test]
fn interceptor_allows_authenticated_requests() -> anyhow::Result<()> {
    let mut interceptor = AuthInterceptor {
        paseto_key: paseto_key_for_tests(),
    };

    let auth_info = AuthInfo { account_id: 55 };
    let auth_token = auth_info.into_auth_token(&interceptor.paseto_key)?;

    let mut request = Request::new(());
    request
        .metadata_mut()
        .insert(AUTH_TOKEN_HEADER, auth_token.parse()?);

    let request = interceptor.call(request)?;

    let roundtrip_auth_info = AuthInfo::from_request(&request)?;

    assert_eq!(roundtrip_auth_info, auth_info);

    Ok(())
}

impl AuthServer {
    pub fn for_tests(db: PgPool) -> Self {
        Self {
            db,
            paseto_key: paseto_key_for_tests(),
        }
    }

    /// Registers and authenticates a synthetic account for testing purposes.
    ///
    /// To make a request originating from this account use [`AuthInfo::request`].
    pub async fn test_account(&self) -> Result<AuthInfo, Status> {
        static TEST_ACCOUNT_COUNTER: AtomicUsize = AtomicUsize::new(0);

        let test_account_id = TEST_ACCOUNT_COUNTER.fetch_add(1, Ordering::Relaxed);

        let username = format!("test-{test_account_id}");
        let password = "coxinha123".to_owned();

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

        Ok(auth_info)
    }
}

impl AuthInfo {
    /// Creates a [`Request`] containing the current auth info in it's extensions, making it look like it was requested
    /// by this account through the normal [`AuthInterceptor`] flow.
    pub fn request<T>(&self, value: T) -> Request<T> {
        let mut request = Request::new(value);
        request.extensions_mut().insert(*self);
        request
    }
}
