use tonic::Status;

pub fn error() -> impl Fn(sqlx::Error) -> Status {
    move |error| {
        tracing::error!(%error, "Database error");
        Status::internal("Database error")
    }
}
