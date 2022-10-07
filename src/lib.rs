//! Implementation of a web-crawler library.
//!
//! The crawler should take as input a webpage URL
//! traverse the contained links in a breadth-first manner
//! and save each page into a folder.
//!
//! The maximum number of pages that should be crawled is 100.
//!
//! The minimum number of parallel tasks should be 5 and the maximum
//! number should be 50.
use futures::stream::{FuturesOrdered, StreamExt};
use scraper::{Html, Selector};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use error::Result;

pub mod error;

const MAX_PAGES: usize = 100;
const MIN_TASKS: usize = 5;
const MAX_TASKS: usize = 50;

pub struct Crawler;

impl Crawler {
    pub fn scrape(page: String) -> Vec<url::Url> {
        let html = Html::parse_document(&page);
        let selector = Selector::parse("a").unwrap();
        html.select(&selector)
            .filter_map(|element| element.value().attr("href"))
            .filter_map(|href| url::Url::parse(href).ok())
            .collect()
    }

    pub async fn visit(url: impl AsRef<str>, tx: mpsc::Sender<String>) -> Result<()> {
        let body = reqwest::get(url.as_ref()).await?.text().await?;
        for url in Self::scrape(body) {
            tx.send(url.into()).await.unwrap();
        }
        Ok(())
    }

    pub async fn run(root_url: String, n_tasks: Option<usize>) {
        let n_tasks = n_tasks.unwrap_or(MIN_TASKS).min(MAX_TASKS);
        let mut tasks = FuturesOrdered::new();
        let (tx, rx) = mpsc::channel(n_tasks);
        let mut rx = ReceiverStream::new(rx).fuse();
        let sender = tx.clone();
        tasks.push_back(tokio::task::spawn(Self::visit(root_url, sender)));
        let mut i = 1;
        while i < MAX_PAGES {
            tokio::select!(
                _ = tasks.next() => {},
                url = rx.next() => {
                    i += 1;
                    let url = url.unwrap();
                    println!("==> Visiting {}th url: {:?}", i, url);
                    let sender = tx.clone();
                    tasks.push_back(tokio::task::spawn(Self::visit(url, sender)));
                },
                else => break
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn it_works() {
        Crawler::run("http://www.spiegel.de".into(), Some(50)).await;
    }
}
