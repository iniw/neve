use tonic::Status;

pub fn db(error: sqlx::Error) -> Status {
    tracing::error!(%error, "Database error");
    Status::internal("Database error")
}
