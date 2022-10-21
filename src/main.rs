use clap::Parser;
use tracing::info;
use tracing_subscriber::FmtSubscriber;
use webcrawler::{Crawler, Scraper};

const MAX_PAGES: usize = 100;
const MIN_TASKS: usize = 5;

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "_", env!("CARGO_PKG_VERSION"),);

fn use_tracing_subscriber() {
    let subscriber = FmtSubscriber::builder().finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}

/// A command-line application that launches a crawler
/// starting from a root url, and descending to nested
/// urls in a breadth-first manner.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
    /// The root url to start the crawling from
    root_url: String,

    /// Max number of concurrent tasks to trigger
    #[arg(long, default_value_t = MIN_TASKS)]
    max_tasks: usize,

    /// Max number of pages to visit
    #[arg(long, default_value_t = MAX_PAGES)]
    max_pages: usize,

    /// Number of workers. By default this equals
    /// the number of available cores.
    #[arg(long)]
    n_workers: Option<usize>,
}

fn main() -> webcrawler::error::Result<()> {
    use_tracing_subscriber();
    env_logger::init();

    let args = CliArgs::parse();

    let max_tasks = args.max_tasks;
    let max_pages = args.max_pages;

    let client = reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .build()?;

    let mut rt_builder = tokio::runtime::Builder::new_multi_thread();
    if let Some(n_workers) = args.n_workers {
        rt_builder.worker_threads(n_workers);
    }
    rt_builder
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move {
            info!("==> Starting crawler...");
            Crawler::new(args.root_url, None, Some(Scraper::new(client)))?
                .run(max_tasks, max_pages)
                .await
        })
}
