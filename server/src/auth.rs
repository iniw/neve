use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use sqlx::PgPool;
use tokio::sync::RwLock;
use tonic::{
    Request, Response, Status,
    body::Body,
    codegen::http::{HeaderValue, Request as HttpRequest},
};
use tonic_middleware::{InterceptorFor, RequestInterceptor};
use tracing::{Level, info, instrument};

use neve_proto::{
    AUTH_TOKEN_HEADER,
    server::v1::{
        AuthenticateRequest, AuthenticateResponse, RegisterRequest, RegisterResponse,
        auth_service_server::{AuthService, AuthServiceServer},
    },
};
use neve_server::RowId;

use crate::error;

#[cfg(test)]
pub mod tests;

#[derive(derive_more::Debug)]
pub struct AuthServer {
    #[debug(skip)]
    db: PgPool,

    #[debug(skip)]
    auth_db: AuthDb,

    auth_id: AtomicU64,
}

impl AuthServer {
    pub fn new(db: PgPool) -> Self {
        Self {
            db,
            auth_db: Default::default(),
            auth_id: Default::default(),
        }
    }

    pub fn service(self) -> AuthServiceServer<Self> {
        AuthServiceServer::new(self)
    }

    /// Wraps the given service in the [`AuthInterceptor`] interceptor.
    pub fn interceptor<S>(&self, service: S) -> InterceptorFor<S, AuthInterceptor> {
        InterceptorFor::new(
            service,
            AuthInterceptor {
                auth_db: self.auth_db.clone(),
            },
        )
    }
}

#[tonic::async_trait]
impl AuthService for AuthServer {
    #[instrument(err)]
    async fn register(
        &self,
        request: Request<RegisterRequest>,
    ) -> Result<Response<RegisterResponse>, Status> {
        let RegisterRequest { username, password } = request.into_inner();

        let account = sqlx::query!(
            r#"
                INSERT INTO account (username, password)
                VALUES ($1, $2)
                RETURNING id
            "#,
            username,
            password
        )
        .fetch_one(&self.db)
        .await
        .map_err(|error| {
            // Specialize the error for username collisions to provide a better error message,
            // since it's not really an internal error.
            if let sqlx::Error::Database(db_error) = &error
                && db_error.is_unique_violation()
            {
                Status::already_exists("Username is already registered")
            } else {
                error::db(error)
            }
        })?;

        info!(?account, ?username, ?password, "Registered account");

        Ok(Response::new(RegisterResponse {}))
    }

    #[instrument(err)]
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
        .map_err(error::db)?
        .ok_or(Status::not_found("Username not found"))?;

        if account.password != password {
            return Err(Status::unauthenticated("Incorrect password"));
        }

        let auth_token = self.auth_id.fetch_add(1, Ordering::Relaxed);

        let auth_token_header = HeaderValue::from_str(&auth_token.to_string())
            .expect("A u64 is always a valid header value");

        self.auth_db
            .write()
            .await
            .insert(auth_token_header, account.id);

        let auth_token = auth_token.to_string();

        Ok(Response::new(AuthenticateResponse { auth_token }))
    }
}

/// An [interceptor](tonic_middleware::RequestInterceptor) that ensures every HTTP request received contains valid
/// authentication metadata, presumably obtained through the [`AuthService::authenticate`] RPC method.
///
/// Requests with missing/invalid credentials will fail with status code [`Unauthenticated`](tonic::Code::Unauthenticated).
///
/// Requests with valid credentials will be augmented with [authentication-related information](`AuthInfo`) that the RPC
/// handler can use to determine which account performed the request.
#[derive(Clone, derive_more::Debug)]
pub struct AuthInterceptor {
    #[debug(skip)]
    auth_db: AuthDb,
}

#[tonic::async_trait]
impl RequestInterceptor for AuthInterceptor {
    #[instrument(level = Level::TRACE, err(level = Level::WARN))]
    async fn intercept(&self, mut request: HttpRequest<Body>) -> Result<HttpRequest<Body>, Status> {
        let auth_token = request
            .headers()
            .get(AUTH_TOKEN_HEADER)
            .ok_or(Status::unauthenticated("Missing authentication token"))?;

        let account_id = self
            .auth_db
            .read()
            .await
            .get(auth_token)
            .copied()
            .ok_or(Status::unauthenticated("Invalid authentication token"))?;

        request.extensions_mut().insert(AuthInfo { account_id });

        Ok(request)
    }
}

/// Authentication-related information that [`AuthInterceptor`] adds to every correctly authenticated [`Request`].
#[derive(Clone, Copy)]
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
}

type AuthDb = Arc<RwLock<HashMap<HeaderValue, RowId>>>;
