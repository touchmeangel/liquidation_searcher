use anyhow::Context;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Config {
  pub(crate) ws_url: String,
  pub(crate) redis_url: String,
  pub(crate) pubsub_url: String,
}

impl Config {
  pub fn open() -> anyhow::Result<Config> {
    let _ = dotenvy::dotenv();
    let ws_url = std::env::var("WS_URL").context("\"WS_URL\" is required")?;
    let redis_url = std::env::var("REDIS_CONNECTION").context("\"REDIS_CONNECTION\" is required")?;
    let pubsub_url = std::env::var("PUBSUB_CONNECTION").context("\"PUBSUB_CONNECTION\" is required")?;
    let config = Config {
      ws_url,
      redis_url,
      pubsub_url,
    };

    Ok(config)
  }
}