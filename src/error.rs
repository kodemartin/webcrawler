//! Library-specific errors
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CrawlerError {
    #[error("sync error {0}")]
    UrlSend(#[from] tokio::sync::mpsc::error::SendError<url::Url>),
    #[error("url with no host")]
    NoUrlHost,
    #[error("url parse error {0}")]
    UrlParse(#[from] url::ParseError),
    #[error("{0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("io error {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, CrawlerError>;
