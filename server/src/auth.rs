use std::{sync::Arc, time::Duration};

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use rusty_paseto::{
    Error as PasetoError,
    core::{Local, V4},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tonic::{
    Request, Response, Status,
    service::{Interceptor, interceptor::InterceptedService},
};
use tracing::{Level, instrument};

use neve_proto::{
    AUTH_TOKEN_HEADER,
    server::v1::{
        AuthenticateRequest, AuthenticateResponse, RegisterRequest, RegisterResponse,
        auth_service_server::{AuthService, AuthServiceServer},
    },
};
use neve_server::RowId;

use crate::error::IntoStatus;

#[cfg(test)]
pub mod tests;

pub struct AuthServer {
    db: PgPool,
    paseto_key: Arc<PasetoSymmetricKey>,
}

impl AuthServer {
    pub fn new(db: PgPool, paseto_key: PasetoKey) -> Self {
        Self {
            db,
            paseto_key: Arc::new(PasetoSymmetricKey::from(paseto_key)),
        }
    }

    pub fn service(self) -> AuthServiceServer<Self> {
        AuthServiceServer::new(self)
    }

    /// Wraps the given service in the [`AuthInterceptor`] interceptor.
    pub fn interceptor<S>(&self, service: S) -> InterceptedService<S, AuthInterceptor> {
        InterceptedService::new(
            service,
            AuthInterceptor {
                paseto_key: self.paseto_key.clone(),
            },
        )
    }
}

#[tonic::async_trait]
impl AuthService for AuthServer {
    #[instrument(skip(self, request), fields(request.username = ?request.get_ref().username), err)]
    async fn register(
        &self,
        request: Request<RegisterRequest>,
    ) -> Result<Response<RegisterResponse>, Status> {
        let RegisterRequest { username, password } = request.into_inner();

        let hashed_password = Self::hash_password(password).await?;

        sqlx::query!(
            r#"
                INSERT INTO account (username, password)
                VALUES ($1, $2)
            "#,
            username,
            hashed_password
        )
        .execute(&self.db)
        .await
        .map_err(|error| {
            // Specialize the error for username collisions to provide a better error message,
            // since it's not really an internal error.
            if let sqlx::Error::Database(db_error) = &error
                && db_error.is_unique_violation()
            {
                Status::already_exists("Username is already registered")
            } else {
                error.into_status()
            }
        })?;

        Ok(Response::new(RegisterResponse {}))
    }

    #[instrument(skip(self, request), fields(request.username = ?request.get_ref().username), err)]
    async fn authenticate(
        &self,
        request: Request<AuthenticateRequest>,
    ) -> Result<Response<AuthenticateResponse>, Status> {
        let AuthenticateRequest { username, password } = request.into_inner();

        let account = sqlx::query!(
            r#"
                SELECT id, password
                FROM account
                WHERE username = $1
            "#,
            &username
        )
        .fetch_optional(&self.db)
        .await
        .map_err(IntoStatus::into_status)?
        .ok_or(Status::not_found("Username not found"))?;

        Self::compare_passwords(account.password, password).await?;

        let auth_token = AuthInfo {
            account_id: account.id,
        }
        .into_auth_token(&self.paseto_key)
        .map_err(|_| Status::internal("Failed to build authentication token"))?;

        Ok(Response::new(AuthenticateResponse { auth_token }))
    }
}

/// An [`Interceptor`] that ensures every HTTP request received contains valid
/// authentication metadata, presumably obtained through the [`AuthService::authenticate`] RPC method.
///
/// Requests with missing/invalid credentials will fail with status code [`Unauthenticated`](tonic::Code::Unauthenticated).
///
/// Requests with valid credentials will be augmented with [authentication-related information](`AuthInfo`) that the RPC
/// handler can use to determine which account performed the request.
#[derive(Clone)]
pub struct AuthInterceptor {
    paseto_key: Arc<PasetoSymmetricKey>,
}

impl Interceptor for AuthInterceptor {
    #[instrument(skip(self), level = Level::TRACE, err(level = Level::WARN))]
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let auth_token = request
            .metadata()
            .get(AUTH_TOKEN_HEADER)
            .ok_or(Status::unauthenticated("Missing authentication token"))?
            .to_str()
            .map_err(|_| Status::unauthenticated("Invalid authentication token encoding"))?;

        let auth_info = AuthInfo::from_auth_token(auth_token, &self.paseto_key)
            .map_err(|_| Status::unauthenticated("Invalid authentication token"))?;

        request.extensions_mut().insert(auth_info);

        Ok(request)
    }
}

/// Authentication-related information that [`AuthInterceptor`] adds to every correctly authenticated [`Request`].
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct AuthInfo {
    /// The ID of the account that performed the request.
    pub account_id: RowId,
}

impl AuthInfo {
    /// Obtains the [`AuthInfo`] stored in a [`Request`]'s [`Extensions`](tonic::Extensions) by [`AuthInterceptor`].
    pub fn from_request<T>(request: &Request<T>) -> Result<Self, Status> {
        request
            .extensions()
            .get::<Self>()
            .copied()
            .ok_or(Status::unauthenticated("No authentication info"))
    }

    /// The claim key used to store the authentication info.
    ///
    /// Must never match one of the [reserved claims](https://github.com/paseto-standard/paseto-spec/blob/master/docs/02-Implementation-Guide/04-Claims.md).
    const CLAIM_KEY: &str = "neve-auth-info";

    /// Builds an authentication token for the current authentication information using the given `paseto_key`
    fn into_auth_token(self, paseto_key: &PasetoSymmetricKey) -> Result<String, PasetoError> {
        PasetoBuilder::default()
            .expires_in(Duration::from_hours(24 * 7))
            .claim(Self::CLAIM_KEY, self)
            .expect("Claim key must not be reserved")
            .build(paseto_key)
            .map_err(PasetoError::from)
    }

    /// Parses an authentication token and returns its authentication information.
    ///
    /// The token must be valid for `paseto_key` and contain an integer `uid` claim that fits in a [`RowId`].
    fn from_auth_token(
        auth_token: &str,
        paseto_key: &PasetoSymmetricKey,
    ) -> Result<Self, PasetoError> {
        let json = PasetoParser::default()
            .parse(auth_token, paseto_key)
            .map_err(PasetoError::from)?;

        let auth_info = json
            .get(Self::CLAIM_KEY)
            .ok_or(PasetoError::MissingClaim(Self::CLAIM_KEY.to_owned()))?;

        Self::deserialize(auth_info)
            .map_err(|_| PasetoError::UnexpectedClaimType(Self::CLAIM_KEY.to_owned()))
    }
}

impl AuthServer {
    /// Hashes a plain text password using [`argon2`].
    async fn hash_password(password: String) -> Result<String, Status> {
        tokio::task::spawn_blocking(move || {
            let salt = SaltString::generate(&mut OsRng);
            Argon2::default()
                .hash_password(password.as_bytes(), &salt)
                .map(|hashed| hashed.to_string())
        })
        .await
        .map_err(|_| Status::internal("Internal panic"))?
        .map_err(|error| {
            tracing::error!(?error);
            Status::internal("Failed to hash password")
        })
    }

    /// Compares an [`argon2`]-hashed password against a plain text password.
    async fn compare_passwords(password: String, against: String) -> Result<(), Status> {
        tokio::task::spawn_blocking(move || {
            let hash = PasswordHash::new(&password)?;
            Argon2::default().verify_password(against.as_bytes(), &hash)
        })
        .await
        .map_err(|_| Status::internal("Internal panic"))?
        .map_err(|_| Status::unauthenticated("Incorrect password"))
    }
}

type PasetoBuilder<'a> = rusty_paseto::prelude::PasetoBuilder<'a, V4, Local>;
type PasetoParser<'a> = rusty_paseto::prelude::PasetoParser<'a, V4, Local>;
type PasetoSymmetricKey = rusty_paseto::core::PasetoSymmetricKey<V4, Local>;
pub type PasetoKey = rusty_paseto::core::Key<32>;
