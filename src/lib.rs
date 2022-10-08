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
use scraper::{Html, Selector};
use sha1::{Digest, Sha1};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use error::{CrawlerError, Result};

pub mod error;

pub struct Crawler {
    root_url: url::Url,
    storage: Arc<Storage>,
}

#[derive(Debug)]
pub struct Storage {
    path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct TaskContext((url::Url, mpsc::Sender<TaskContext>));

impl Storage {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn normalize_url(&self, url: url::Url) -> PathBuf {
        let hash = Sha1::digest(url.as_str().as_bytes());
        let mut path = PathBuf::from(hex::encode(hash.as_slice()));
        path.set_extension("html");
        path
    }

    pub async fn serialize(&self, page: impl AsRef<[u8]>, url: url::Url) -> Result<()> {
        let path = self.path.join(self.normalize_url(url));
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
        Ok(Self { root_url, storage })
    }

    pub fn scrape(page: String) -> Vec<url::Url> {
        let html = Html::parse_document(&page);
        let selector = Selector::parse("a").unwrap();
        html.select(&selector)
            .filter_map(|element| element.value().attr("href"))
            .filter_map(|href| url::Url::parse(href).ok())
            .collect()
    }

    pub async fn visit(
        url: url::Url,
        tx: mpsc::Sender<TaskContext>,
        storage: Arc<Storage>,
    ) -> Result<()> {
        tracing::info!("==> Visiting url: {:?}", url.as_str());
        let body = reqwest::get(url.as_str()).await?.text().await?;
        tracing::info!("  -> Serializing");
        storage.serialize(&body, url).await?;
        tracing::info!("  -> Scraping");
        for url in Self::scrape(body) {
            let new_tx = tx.clone();
            tx.send(TaskContext((url, new_tx))).await?;
        }
        Ok(())
    }

    pub async fn run(self, max_tasks: usize, max_pages: usize) -> Result<()> {
        // Setup storage dir
        tokio::fs::create_dir_all(&self.storage.path).await?;
        // Setup crawler sync
        let mut tasks = FuturesOrdered::new();
        let (tx, rx) = mpsc::channel(2_usize.pow(16));
        let mut rx = ReceiverStream::new(rx).fuse();
        // Start with root url
        let root_url = self.root_url.clone();
        let storage = Arc::clone(&self.storage);
        tasks.push_back(tokio::spawn(Self::visit(root_url, tx, storage)));
        // Descend into nested urls
        let mut n_tasks_remaining = max_tasks - 1;
        let mut n_pages = 1;
        loop {
            tokio::select!(
                Some(result) = &mut tasks.next() => {
                    match result {
                        Ok(Ok(_)) => { n_tasks_remaining += 1 },
                        err => { tracing::warn!("error visiting page: {:?}", err) }
                    }
                },
                Some(TaskContext((url, tx))) = rx.next(), if n_tasks_remaining > 0 && n_pages < max_pages  => {
                    let storage = Arc::clone(&self.storage);
                    tasks.push_back(tokio::spawn(Self::visit(url, tx, storage)));
                    n_tasks_remaining -= 1;
                    n_pages += 1;
                },
                else => break
            );
        }
        Ok(())
    }
}
