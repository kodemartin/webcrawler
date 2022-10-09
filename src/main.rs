use clap::Parser;
use tracing::info;
use tracing_subscriber::FmtSubscriber;
use webcrawler::Crawler;

const MAX_PAGES: usize = 100;
const MIN_TASKS: usize = 5;
const MAX_TASKS: usize = 50;

fn use_tracing_subscriber() {
    let subscriber = FmtSubscriber::builder()
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
    /// The root url to start the crawling from
    root_url: String,

    /// Max number of tasks to trigger
    #[arg(long, default_value_t = MIN_TASKS)]
    max_tasks: usize,

    /// Max number of pages to visit
    #[arg(long, default_value_t = 100)]
    max_pages: usize,
}

#[tokio::main]
async fn main() -> webcrawler::error::Result<()> {
    use_tracing_subscriber();
	env_logger::init();

    let args = CliArgs::parse();

    let max_tasks = args.max_tasks.min(MAX_TASKS);
    let max_pages = args.max_pages.min(MAX_PAGES);

    info!("==> Starting crawler...");
    Crawler::new(args.root_url, None)?
        .run(max_tasks, max_pages)
        .await
}
