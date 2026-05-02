use anyhow::Context;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Config {
  pub(crate) http_url: String,
  pub(crate) ws_url: String,
  pub(crate) pubsub_url: String,
  pub(crate) capacity: usize,
  pub(crate) accounts_batch_size: usize
}

impl Config {
  pub fn open() -> anyhow::Result<Config> {
    let _ = dotenvy::dotenv();
    let http_url = std::env::var("HTTP_URL").context("\"HTTP_URL\" is required")?;
    let ws_url = std::env::var("WS_URL").context("\"WS_URL\" is required")?;
    let pubsub_url = std::env::var("PUBSUB_CONNECTION").context("\"PUBSUB_CONNECTION\" is required")?;
    let capacity = env_usize("CAPACITY", 1).context("invalid \"CAPACITY\" value")?;
    let accounts_batch_size = env_usize("ACCOUNTS_BATCH_SIZE", 1000).context("invalid \"ACCOUNTS_BATCH_SIZE\" value")?;
    let config = Config {
      http_url,
      ws_url,
      pubsub_url,
      capacity,
      accounts_batch_size
    };

    Ok(config)
  }
}

fn env_usize(name: &str, default: usize) -> Result<usize, std::num::ParseIntError> {
  std::env::var(name)
    .ok()
    .filter(|s| !s.is_empty())
    .map(|s| s.parse::<usize>())
    .transpose()
    .map(|opt| opt.unwrap_or(default))
}