//! Library-specific errors
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CrawlerError {
    #[error("{0}")]
    Reqwest(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, CrawlerError>;
