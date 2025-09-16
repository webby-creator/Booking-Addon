use webby_addon_common::WrappingResponse;
use axum::response::{IntoResponse, Json, Response};
use hyper::StatusCode;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("From UTF8 Error: {0}")]
    FromUtf8(#[from] std::string::FromUtf8Error),

    #[error("IO Error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Parse int Error: {0}")]
    ParseInt(#[from] std::num::ParseIntError),

    #[error("Serde JSON Error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("Eyre Error: {0}")]
    Eyre(#[from] eyre::Report),

    #[error("UUID Error: {0}")]
    UUID(#[from] uuid::Error),
    #[error("Time Error: {0}")]
    Time(#[from] time::Error),
    #[error("Time Error: {0}")]
    TimeRange(#[from] time::error::ComponentRange),
    #[error("Time Parse Error: {0}")]
    TimeParse(#[from] time::error::Parse),

    #[error("Multipart Error: {0}")]
    Multipart(#[from] axum::extract::multipart::MultipartError),
    #[error("Axum Error: {0}")]
    Axum(#[from] axum::Error),

    #[error("Convert PathBuf to String Error")]
    ConvertPathBufToString,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(WrappingResponse::<()>::error(self.to_string())),
        )
            .into_response()
    }
}
