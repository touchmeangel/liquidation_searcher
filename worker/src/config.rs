use anyhow::Context;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Config {
  pub(crate) http_url: String,
  pub(crate) ws_url: String,
  pub(crate) pubsub_url: String,
  pub(crate) accounts_batch_size: usize
}

impl Config {
  pub fn open() -> anyhow::Result<Config> {
    let _ = dotenvy::dotenv();
    let http_url = std::env::var("HTTP_URL").context("\"HTTP_URL\" is required")?;
    let ws_url = std::env::var("WS_URL").context("\"WS_URL\" is required")?;
    let pubsub_url = std::env::var("PUBSUB_CONNECTION").context("\"PUBSUB_CONNECTION\" is required")?;
    let accounts_batch_size_str = std::env::var("ACCOUNTS_BATCH_SIZE");
    let accounts_batch_size = accounts_batch_size_str.map(|s| s.parse::<usize>()).unwrap_or(Ok(1000))?;
    let config = Config {
      http_url,
      ws_url,
      pubsub_url,
      accounts_batch_size
    };

    Ok(config)
  }
}