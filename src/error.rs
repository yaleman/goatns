use std::str::Utf8Error;

use packed_struct::PackingError;

/// When things go awry
#[derive(Debug)]
pub enum GoatNsError {
    Csrf(String),
    BytePackingError(String),
    InvalidName,
    InvalidHeader(String),
    IoError(std::io::Error),
    /// Something failed in the start up of the platform
    StartupError(String),
    SqlxError(sqlx::Error),
    Oidc(String),
    SeaOrm(sea_orm::DbErr),
    ReqwestError(reqwest::Error),
    FileError(String),
    EmptyFile,
    /// Failed to send something across a tokio channel
    SendError(String),
    Utf8Error(Utf8Error),
    DateParseError(String),
    /// No ANY records for you!
    RFC8482,
    Generic(String),
    Regex(String),
    InvalidValue(String),
}

impl From<regex::Error> for GoatNsError {
    fn from(error: regex::Error) -> Self {
        GoatNsError::Regex(error.to_string())
    }
}
impl From<std::io::Error> for GoatNsError {
    fn from(error: std::io::Error) -> Self {
        GoatNsError::IoError(error)
    }
}

impl From<sqlx::Error> for GoatNsError {
    fn from(error: sqlx::Error) -> Self {
        GoatNsError::SqlxError(error)
    }
}

impl From<reqwest::Error> for GoatNsError {
    fn from(error: reqwest::Error) -> Self {
        GoatNsError::ReqwestError(error)
    }
}

impl From<PackingError> for GoatNsError {
    fn from(error: PackingError) -> Self {
        GoatNsError::BytePackingError(error.to_string())
    }
}

impl From<Utf8Error> for GoatNsError {
    fn from(error: Utf8Error) -> Self {
        GoatNsError::Utf8Error(error)
    }
}

impl From<chrono::format::ParseError> for GoatNsError {
    fn from(error: chrono::format::ParseError) -> Self {
        GoatNsError::DateParseError(error.to_string())
    }
}

impl From<sea_orm::DbErr> for GoatNsError {
    fn from(error: sea_orm::DbErr) -> Self {
        GoatNsError::SeaOrm(error)
    }
}

impl From<GoatNsError> for std::io::Error {
    fn from(error: GoatNsError) -> Self {
        match error {
            GoatNsError::IoError(err) => err,
            GoatNsError::StartupError(err) => std::io::Error::new(std::io::ErrorKind::Other, err),
            GoatNsError::SqlxError(err) => std::io::Error::new(std::io::ErrorKind::Other, err),
            GoatNsError::ReqwestError(err) => std::io::Error::new(std::io::ErrorKind::Other, err),
            GoatNsError::FileError(err) => std::io::Error::new(std::io::ErrorKind::Other, err),
            GoatNsError::EmptyFile => std::io::Error::new(std::io::ErrorKind::Other, "Empty file"),
            GoatNsError::SendError(err) => std::io::Error::new(std::io::ErrorKind::Other, err),
            _ => std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", error)),
        }
    }
}
