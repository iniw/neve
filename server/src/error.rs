use tonic::Status;

pub fn db() -> impl Fn(sqlx::Error) -> Status {
    move |error| {
        tracing::error!(%error, "Database error");
        Status::internal("Database error")
    }
}
