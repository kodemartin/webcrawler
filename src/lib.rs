//! Implementation of a web-crawler library.
//!
//! The crawler takes as input a root webpage URL and
//! traverses the contained links in a breadth-first manner.
//!
//! Each visited page is stored in the disk.
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use futures::stream::{FuturesOrdered, StreamExt};
use scraper::{Html, Selector};
use sha1::{Digest, Sha1};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_stream::wrappers::ReceiverStream;

use error::{CrawlerError, Result};

pub mod error;

pub struct Crawler {
    root_url: url::Url,
    storage: Arc<Storage>,
    scraper: Scraper,
    visited: HashSet<url::Url>,
    task_queue: FuturesOrdered<JoinHandle<Result<()>>>,
}

impl Crawler {
    pub fn new(
        root_url: String,
        storage: Option<Storage>,
        scraper: Option<Scraper>,
    ) -> Result<Self> {
        let root_url = url::Url::parse(&root_url)?;
        let storage = match storage {
            Some(storage) => Arc::new(storage),
            None => Arc::new(Storage::try_from(&root_url)?),
        };
        let visited = HashSet::default();
        let scraper = scraper.unwrap_or_default();
        let task_queue = FuturesOrdered::new();
        Ok(Self {
            root_url,
            storage,
            scraper,
            visited,
            task_queue,
        })
    }

    pub fn queue_task(&mut self, url: url::Url, tx: mpsc::Sender<TaskContext>) {
        let storage = Arc::clone(&self.storage);
        let scraper = self.scraper.clone();
        self.task_queue.push_back(tokio::spawn(async move {
            scraper.visit(url, tx, storage).await
        }));
    }

    pub async fn run(mut self, max_tasks: usize, max_pages: usize) -> Result<()> {
        // Setup storagedir
        self.storage.setup().await?;
        // Setup crawler sync
        let (tx, rx) = mpsc::channel(2_usize.pow(16));
        let mut rx = ReceiverStream::new(rx).fuse();
        // Start with root url
        let root_url = self.root_url.clone();
        self.queue_task(root_url, tx);
        self.visited.insert(self.root_url.clone());
        // Descend into nested urls
        let mut n_tasks_remaining = max_tasks - 1;
        let mut n_pages_queued = 1;
        let mut n_pages_visited = 0;
        loop {
            tokio::select!(
                Some(result) = &mut self.task_queue.next() => {
                    match result {
                        Ok(Ok(_)) => {
                            n_pages_visited += 1;
                            n_tasks_remaining += 1;
                            tracing::info!("==> Visited {} out of {}", n_pages_visited, max_pages);
                        },
                        err => {
                            n_pages_queued -= 1;
                            tracing::warn!("error visiting page: {:?}", err);
                        }
                    }
                },
                Some(TaskContext((url, tx))) = rx.next(), if n_tasks_remaining > 0 && n_pages_queued < max_pages  => {
                    if !&self.visited.contains(&url) {
                        self.visited.insert(url.clone());
                        self.queue_task(url, tx);
                        n_tasks_remaining -= 1;
                        n_pages_queued += 1;
                    }
                },
                else => break
            );
        }
        Ok(())
    }
}

/// Context for spawning a crawl task
#[derive(Debug, Clone)]
pub struct TaskContext((url::Url, mpsc::Sender<TaskContext>));

/// The storage for persisting webpages
#[derive(Debug)]
pub struct Storage {
    path: PathBuf,
}

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

/// Encapsulates functionality to get the webpage
/// and scrape the desired information
#[derive(Default, Clone)]
pub struct Scraper {
    pub client: reqwest::Client,
}

impl Scraper {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
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
        &self,
        url: url::Url,
        tx: mpsc::Sender<TaskContext>,
        storage: Arc<Storage>,
    ) -> Result<()> {
        tracing::debug!("==> Visiting url: {:?}", url.as_str());
        let body = self.client.get(url.as_str()).send().await?.text().await?;
        tracing::debug!("  -> Serializing");
        storage.serialize(&body, &url).await?;
        tracing::debug!("  -> Scraping");
        for url in Self::scrape(body) {
            let new_tx = tx.clone();
            tx.send(TaskContext((url, new_tx))).await?;
        }
        Ok(())
    }
}
