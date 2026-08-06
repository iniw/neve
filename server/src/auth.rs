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

use neve_proto::{
    AUTH_TOKEN_HEADER,
    server::v1::{
        AuthenticateRequest, AuthenticateResponse, RegisterRequest, RegisterResponse,
        auth_service_server::{AuthService, AuthServiceServer},
    },
};

use crate::db;

type RowId = i64;
type AuthDb = Arc<RwLock<HashMap<HeaderValue, RowId>>>;

pub struct AuthServer {
    db: PgPool,

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

    pub fn auth_interceptor<S>(&self, service: S) -> InterceptorFor<S, AuthInterceptor> {
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
    async fn register(
        &self,
        _request: Request<RegisterRequest>,
    ) -> Result<Response<RegisterResponse>, Status> {
        todo!()
    }

    #[tracing::instrument(skip_all, err)]
    async fn authenticate(
        &self,
        request: Request<AuthenticateRequest>,
    ) -> Result<Response<AuthenticateResponse>, Status> {
        let AuthenticateRequest { username, password } = request.into_inner();

        let account = sqlx::query!(
            r#"
                SELECT *
                FROM account
                WHERE username = $1
                LIMIT 1
            "#,
            &username
        )
        .fetch_optional(&self.db)
        .await
        .map_err(db::error())?
        .ok_or(Status::not_found("Username not found"))?;

        if account.password != password {
            return Err(Status::unauthenticated("Incorrect password"));
        }

        let auth_token = self.auth_id.fetch_add(1, Ordering::Relaxed);

        let auth_token_header = HeaderValue::from_bytes(auth_token.to_ne_bytes().as_slice())
            .expect("A u64 is always a valid header value");

        self.auth_db
            .write()
            .await
            .insert(auth_token_header, account.id);

        let auth_token = auth_token.to_string();

        Ok(Response::new(AuthenticateResponse { auth_token }))
    }
}

#[derive(Clone)]
pub struct AuthInterceptor {
    auth_db: AuthDb,
}

#[tonic::async_trait]
impl RequestInterceptor for AuthInterceptor {
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

#[derive(Clone, Copy)]
pub struct AuthInfo {
    pub account_id: RowId,
}
