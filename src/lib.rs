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
    pub async fn visit(url: impl AsRef<str>, tx: mpsc::Sender<String>) -> Result<()> {
        let body = reqwest::get(url.as_ref()).await?.text().await?;
        let html = Html::parse_document(&body);
        let selector = Selector::parse("a").unwrap();
        let mut links = html.select(&selector);
        //println!("  -> Parsing links of {:?}", url.as_ref());
        while let Some(url) = links
            .next()
            .and_then(|a| a.value().attr("href"))
            .map(url::Url::parse)
        {
            if let Ok(url) = url {
                tx.send(url.into()).await.unwrap();
            }
        }
        Ok(())
    }

    pub async fn run(root_url: String, n_tasks: Option<usize>) {
        let n_tasks = n_tasks.unwrap_or(MIN_TASKS).min(MAX_TASKS);
        let mut tasks = FuturesOrdered::new();
        let (tx, rx) = mpsc::channel(2_usize.pow(16));
        let mut rx = ReceiverStream::new(rx).fuse();
        tasks.push_back(tokio::task::spawn(async move { Self::visit(root_url, tx.clone()).await; }));
        let mut i = 1;
        let mut j = n_tasks - 1;
        while i < MAX_PAGES {
            tokio::select!(
                _ = tasks.next() => { j += 1; },
                url = rx.next(), if j > 0 => {
                    i += 1;
                    let url = url.unwrap();
                    println!("==> Visiting {}th url: {:?}", i, url);
                    tasks.push_back(tokio::task::spawn(async move { Self::visit(url, tx.clone()).await; }));
                    j -= 1;
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
        Crawler::run("http://www.spiegel.de".into(), Some(5)).await;
    }
}
