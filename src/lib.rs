//! Implementation of a web-crawler library.
//!
//! The crawler takes as input a webpage URL, a maximum number of concurrent
//! tasks to spawn, and a maximum number of pages to visit.
//!
//! It then traverses the contained links in a breadth-first manner
//! and saves each page into a folder.
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use futures::stream::{FuturesOrdered, StreamExt};
use scraper::{Html, Selector};
use sha1::{Digest, Sha1};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use error::{CrawlerError, Result};

pub mod error;

/// Represent the crawler
pub struct Crawler {
    root_url: url::Url,
    storage: Arc<Storage>,
    visited: HashSet<url::Url>,
}

/// The storage for persisting webpages
#[derive(Debug)]
pub struct Storage {
    path: PathBuf,
}

/// Context for spawing a crawl task
#[derive(Debug, Clone)]
pub struct TaskContext((url::Url, mpsc::Sender<TaskContext>));

impl Storage {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub async fn setup(&self) -> Result<()> {
        Ok(tokio::fs::create_dir_all(&self.path).await?)
    }

    pub fn url_to_path(&self, url: &url::Url) -> PathBuf {
        let hash = Sha1::digest(url.as_str().as_bytes());
        let mut path = PathBuf::from(hex::encode(hash.as_slice()));
        path.set_extension("html");
        path
    }

    pub async fn serialize(&self, page: impl AsRef<[u8]>, url: &url::Url) -> Result<()> {
        let path = self.path.join(self.url_to_path(url));
        tokio::fs::write(path, page).await?;
        Ok(())
    }
}

impl TryFrom<&url::Url> for Storage {
    type Error = CrawlerError;

    fn try_from(url: &url::Url) -> Result<Self> {
        let ts = chrono::Utc::now().timestamp_millis();
        let host = url.host_str().ok_or(CrawlerError::NoUrlHost)?;
        Ok(Storage::new(format!("webpages/{}_{}", host, ts).into()))
    }
}

impl Crawler {
    pub fn new(root_url: String, storage: Option<Storage>) -> Result<Self> {
        let root_url = url::Url::parse(&root_url)?;
        let storage = match storage {
            Some(storage) => Arc::new(storage),
            None => Arc::new(Storage::try_from(&root_url)?),
        };
        let visited = HashSet::default();
        Ok(Self {
            root_url,
            storage,
            visited,
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

    pub async fn visit(
        url: url::Url,
        tx: mpsc::Sender<TaskContext>,
        storage: Arc<Storage>,
    ) -> Result<()> {
        tracing::debug!("==> Visiting url: {:?}", url.as_str());
        let body = reqwest::get(url.as_str()).await?.text().await?;
        tracing::debug!("  -> Serializing");
        storage.serialize(&body, &url).await?;
        tracing::debug!("  -> Scraping");
        for url in Self::scrape(body) {
            let new_tx = tx.clone();
            tx.send(TaskContext((url, new_tx))).await?;
        }
        Ok(())
    }

    pub async fn run(mut self, max_tasks: usize, max_pages: usize) -> Result<()> {
        // Setup storagedir
        self.storage.setup().await?;
        // Setup crawler sync
        let mut tasks = FuturesOrdered::new();
        let (tx, rx) = mpsc::channel(2_usize.pow(16));
        let mut rx = ReceiverStream::new(rx).fuse();
        // Start with root url
        let storage = Arc::clone(&self.storage);
        tasks.push_back(tokio::spawn(Self::visit(
            self.root_url.clone(),
            tx,
            storage,
        )));
        self.visited.insert(self.root_url);
        // Descend into nested urls
        let mut n_tasks_remaining = max_tasks - 1;
        let mut n_pages_qeued = 1;
        let mut n_pages_visited = 0;
        loop {
            tokio::select!(
                Some(result) = &mut tasks.next() => {
                    match result {
                        Ok(Ok(_)) => {
                            n_pages_visited += 1;
                            n_tasks_remaining += 1;
                            tracing::info!("==> Visited {} out of {}", n_pages_visited, max_pages);
                        },
                        err => { tracing::warn!("error visiting page: {:?}", err) }
                    }
                },
                Some(TaskContext((url, tx))) = rx.next(), if n_tasks_remaining > 0 && n_pages_qeued < max_pages  => {
                    if !&self.visited.contains(&url) {
                        self.visited.insert(url.clone());
                        let storage = Arc::clone(&self.storage);
                        tasks.push_back(tokio::spawn(Self::visit(url, tx, storage)));
                        n_tasks_remaining -= 1;
                        n_pages_qeued += 1;
                    }
                },
                else => break
            );
        }
        Ok(())
    }
}
