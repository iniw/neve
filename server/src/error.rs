use std::error::Error;

use tonic::{Code, Status};

/// A trait to conveniently convert an [`Error`] into a [`Status`].
pub trait IntoStatus: Error {
    const MESSAGE: &'static str;
    const STATUS_CODE: Code = Code::Internal;

    fn into_status(self) -> Status
    where
        Self: Sized,
    {
        tracing::error!(error = %self, "{}", Self::MESSAGE);
        Status::new(Self::STATUS_CODE, Self::MESSAGE)
    }
}

impl IntoStatus for sqlx::Error {
    const MESSAGE: &'static str = "Database error";
}
