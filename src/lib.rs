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
use std::path::PathBuf;
use std::sync::Arc;

use futures::stream::{FuturesOrdered, StreamExt};
use sha1::{Sha1, Digest};
use scraper::{Html, Selector};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use error::{CrawlerError, Result};

pub mod error;

const MAX_PAGES: usize = 10;
const MIN_TASKS: usize = 5;
const MAX_TASKS: usize = 50;

pub struct Crawler {
    root_url: url::Url,
    storage: Arc<Storage>,
}

#[derive(Debug)]
pub struct Storage {
    path: PathBuf,
}

impl Storage {

    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
        }
    }

    pub fn normalize_url(&self, url: url::Url) -> PathBuf {
        let hash = Sha1::digest(url.as_str().as_bytes());
        let mut path = PathBuf::from(hex::encode(hash.as_slice()));
        path.set_extension("html");
        path
    }

    pub async fn serialize(&self, page: impl AsRef<[u8]>, url: url::Url) -> Result<()> {
        let path = self.path.join(self.normalize_url(url));
        println!("  -> writing to {:?}", path);
        tokio::fs::write(path, page).await?;
        Ok(())
    }
}

impl Crawler {
    pub fn new(root_url: String) -> Result<Self> {
        let ts = chrono::Utc::now().timestamp_millis();
        let root_url = url::Url::parse(&root_url)?;
        let host = root_url.host_str().ok_or(CrawlerError::NoUrlHost)?;
        let storage = Arc::new(Storage::new(format!("webpages/{}_{}", host, ts).into()));
        Ok(Self {
            root_url,
            storage,
        })
    }

    pub fn scrape(page: String) -> Vec<url::Url> {
        let html = Html::parse_document(&page);
        let selector = Selector::parse("a").unwrap();
        html.select(&selector)
            .filter_map(|element| element.value().attr("href"))
            .filter_map(|href| url::Url::parse(href).ok())
            .collect()
    }

    pub async fn visit(url: url::Url, tx: mpsc::Sender<url::Url>, storage: Arc<Storage>) -> Result<()> {
        println!("==> Visiting url: {:?}", url.as_str());
        let body = reqwest::get(url.as_str()).await?.text().await?;
        println!("  -> Serializing");
        storage.serialize(&body, url).await?;
        println!("  -> Scraping");
        for url in Self::scrape(body) {
            tx.send(url).await.unwrap();
        }
        Ok(())
    }

    pub async fn run(self, n_tasks: Option<usize>) -> Result<()> {
        tokio::fs::create_dir_all(&self.storage.path).await?;
        let n_tasks = n_tasks.unwrap_or(MIN_TASKS).min(MAX_TASKS);
        let mut tasks = FuturesOrdered::new();
        let (tx, rx) = mpsc::channel(2_usize.pow(16));
        let mut rx = ReceiverStream::new(rx).fuse();
        let sender = tx.clone();
        let root_url = self.root_url.clone();
        let storage = Arc::clone(&self.storage);
        tasks.push_back(Self::visit(root_url, sender, storage));
        let mut i = 1;
        let mut j = n_tasks - 1;
        loop {
            println!("{}-{}", i, j);
            tokio::select!(
                Some(res) = tasks.next() => {
                    println!("{:?}", res);
                    j += 1;
                },
                url = rx.next(), if j > 0 && i < MAX_PAGES  => {
                    let sender = tx.clone();
                    let storage = Arc::clone(&self.storage);
                    tasks.push_back(Self::visit(url.unwrap(), sender, storage));
                    j -= 1;
                    i += 1;
                },
                else => break
            )
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn it_works() {
        Crawler::new("https://www.spiegel.de".into())
            .expect("valid root url")
            .run(Some(50)).await;
    }

    #[test]
    fn url() {
        let a = "https://www.spiegel.de/spiegel/";
        let b = "https://www.spiegel.de";
        let a = url::Url::parse(a).unwrap();
        let b = url::Url::parse(b).unwrap();
        println!("{:?}", b.make_relative(&a).expect("").as_str());
    }
}
